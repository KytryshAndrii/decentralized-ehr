use futures::executor::block_on;
use futures::io::Cursor;
use ipfs_bitswap::consts::{MAX_MESSAGE_BYTES, MAX_RESPONSE_BYTES, MAX_WANTLIST_ENTRIES};
use ipfs_bitswap::utils::cid_prefix;
use ipfs_bitswap::{
    BitswapError, BitswapLimits, BitswapMessage, BitswapServer, Block, BlockPresence,
    BlockPresenceType, BlockSource, ProvidedBlockSource, WantType, Wantlist, WantlistEntry,
    read_message, write_message,
};
use ipfs_cid::consts::{CODEC_RAW, SHA2_256};
use ipfs_cid::models::Cid;
use ipfs_store::PersistentBlockstore;
use prost::Message;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct MemorySource {
    blocks: Mutex<HashMap<Vec<u8>, Vec<u8>>>,
    fail: bool,
}
impl MemorySource {
    fn with(blocks: &[&[u8]]) -> Self {
        let source = Self::default();
        for data in blocks {
            source
                .blocks
                .lock()
                .unwrap()
                .insert(cid(data).to_bytes(), data.to_vec());
        }
        source
    }
}
impl BlockSource for MemorySource {
    fn has_blocks(&self, cids: &[Cid]) -> Result<Vec<bool>, BitswapError> {
        if self.fail {
            return Err(BitswapError::Stream("source unavailable".into()));
        }
        let blocks = self.blocks.lock().unwrap();
        Ok(cids
            .iter()
            .map(|cid| blocks.contains_key(&cid.to_bytes()))
            .collect())
    }
    fn get_block(&self, cid: &Cid) -> Result<Option<Vec<u8>>, BitswapError> {
        Ok(self.blocks.lock().unwrap().get(&cid.to_bytes()).cloned())
    }
}
/// Blok znika między planowaniem a wysyłką.
struct VanishingSource;
impl BlockSource for VanishingSource {
    fn has_blocks(&self, cids: &[Cid]) -> Result<Vec<bool>, BitswapError> {
        Ok(vec![true; cids.len()])
    }
    fn get_block(&self, _: &Cid) -> Result<Option<Vec<u8>>, BitswapError> {
        Ok(None)
    }
}

fn cid(data: &[u8]) -> Cid {
    Cid::generate_from_data(data, CODEC_RAW, SHA2_256).unwrap()
}
fn want(cid_bytes: Vec<u8>, want_type: WantType, send_dont_have: bool) -> WantlistEntry {
    WantlistEntry {
        block: cid_bytes,
        priority: 1,
        cancel: false,
        want_type: want_type as i32,
        send_dont_have,
    }
}
fn request(entries: Vec<WantlistEntry>) -> BitswapMessage {
    BitswapMessage {
        wantlist: Some(Wantlist {
            entries,
            full: false,
        }),
        ..BitswapMessage::default()
    }
}
fn presence(cid_bytes: Vec<u8>, kind: BlockPresenceType) -> BlockPresence {
    BlockPresence {
        cid: cid_bytes,
        r#type: kind as i32,
    }
}
fn limits(max_message_bytes: usize, max_response_bytes: usize) -> BitswapLimits {
    BitswapLimits {
        max_message_bytes,
        max_response_bytes,
        ..BitswapLimits::default()
    }
}
/// Wszystkie wiadomości odpowiedzi na jedno żądanie.
fn respond<S: BlockSource>(server: &BitswapServer<S>, req: &BitswapMessage) -> Vec<BitswapMessage> {
    let mut plan = server.plan(req).unwrap();
    let mut messages = Vec::new();
    while let Some(message) = server.next_message(&mut plan).unwrap() {
        messages.push(message);
    }
    messages
}

#[test]
fn protocol_and_default_limits_match_agreed_values() {
    assert_eq!(ipfs_bitswap::BITSWAP_PROTOCOL, "/ipfs/bitswap/1.2.0");
    assert_eq!(MAX_MESSAGE_BYTES, 4 * 1024 * 1024);
    assert_eq!(MAX_WANTLIST_ENTRIES, 100);
    assert_eq!(MAX_RESPONSE_BYTES, 512 * 1024 * 1024);
    let defaults = BitswapLimits::default();
    assert_eq!(defaults.max_message_bytes, MAX_MESSAGE_BYTES);
    assert_eq!(defaults.max_wantlist_entries, MAX_WANTLIST_ENTRIES);
    assert_eq!(defaults.max_response_bytes, MAX_RESPONSE_BYTES);
}
#[test]
fn wantlist_frame_matches_protobuf_wire_format() {
    let message = BitswapMessage {
        wantlist: Some(Wantlist {
            entries: vec![want(vec![0x01, 0x02], WantType::Have, true)],
            full: true,
        }),
        ..BitswapMessage::default()
    };
    // varint długości, potem pola: wantlist(1){ entries(1){ block(1) priority(2)
    // wantType(4) sendDontHave(5) } full(2) }.
    let expected = [
        0x10, 0x0a, 0x0e, 0x0a, 0x0a, 0x0a, 0x02, 0x01, 0x02, 0x10, 0x01, 0x20, 0x01, 0x28, 0x01,
        0x10, 0x01,
    ];
    assert_eq!(message.to_frame(), expected);
    assert_eq!(BitswapMessage::from_frame(&expected).unwrap(), message);
}
#[test]
fn response_fields_use_spec_tags() {
    let message = BitswapMessage {
        payload: vec![Block {
            prefix: vec![0x01],
            data: vec![0xaa],
        }],
        block_presences: vec![presence(vec![0x07], BlockPresenceType::DontHave)],
        ..BitswapMessage::default()
    };
    // payload = pole 3 { prefix(1) data(2) }, blockPresences = pole 4 { cid(1) type(2) }.
    let expected = [
        0x1a, 0x06, 0x0a, 0x01, 0x01, 0x12, 0x01, 0xaa, 0x22, 0x05, 0x0a, 0x01, 0x07, 0x10, 0x01,
    ];
    assert_eq!(message.encode_to_vec(), expected);
}
#[test]
fn malformed_frame_is_rejected() {
    assert!(BitswapMessage::from_frame(&[0x02, 0xff, 0xff]).is_err());
    assert!(BitswapMessage::from_frame(&[0x05, 0x0a]).is_err());
}
#[test]
fn stream_carries_several_messages_then_reports_end() {
    let first = request(vec![want(cid(b"a").to_bytes(), WantType::Block, false)]);
    let second = request(vec![want(cid(b"b").to_bytes(), WantType::Have, true)]);
    let mut wire = Vec::new();
    block_on(async {
        write_message(&mut wire, &first, MAX_MESSAGE_BYTES)
            .await
            .unwrap();
        write_message(&mut wire, &second, MAX_MESSAGE_BYTES)
            .await
            .unwrap();
        let mut reader = Cursor::new(wire);
        assert_eq!(
            read_message(&mut reader, MAX_MESSAGE_BYTES).await.unwrap(),
            Some(first)
        );
        assert_eq!(
            read_message(&mut reader, MAX_MESSAGE_BYTES).await.unwrap(),
            Some(second)
        );
        assert_eq!(
            read_message(&mut reader, MAX_MESSAGE_BYTES).await.unwrap(),
            None
        );
    });
}
#[test]
fn oversized_and_truncated_messages_are_rejected() {
    let message = request(vec![want(cid(b"a").to_bytes(), WantType::Block, false)]);
    block_on(async {
        let mut reader = Cursor::new(message.to_frame());
        assert!(matches!(
            read_message(&mut reader, 8).await,
            Err(BitswapError::MessageTooLarge { limit: 8, .. })
        ));
        let mut frame = message.to_frame();
        frame.truncate(frame.len() - 3);
        let mut reader = Cursor::new(frame);
        assert!(matches!(
            read_message(&mut reader, MAX_MESSAGE_BYTES).await,
            Err(BitswapError::Io(_))
        ));
        let mut wire = Vec::new();
        assert!(matches!(
            write_message(&mut wire, &message, 8).await,
            Err(BitswapError::MessageTooLarge { .. })
        ));
        assert!(wire.is_empty());
    });
}

#[test]
fn cid_prefix_is_cid_without_digest() {
    assert_eq!(cid_prefix(&cid(b"data")), vec![0x01, 0x55, 0x12, 0x20]);
}

#[test]
fn want_block_returns_block_with_prefix() {
    let data = b"ehr test block";
    let server = BitswapServer::new(MemorySource::with(&[data]));
    let req = request(vec![want(cid(data).to_bytes(), WantType::Block, true)]);
    let messages = respond(&server, &req);
    assert_eq!(messages.len(), 1);
    assert!(messages[0].block_presences.is_empty());
    assert_eq!(
        messages[0].payload,
        vec![Block {
            prefix: vec![0x01, 0x55, 0x12, 0x20],
            data: data.to_vec(),
        }]
    );
}
#[test]
fn want_have_returns_have_without_block() {
    let data = b"present";
    let server = BitswapServer::new(MemorySource::with(&[data]));
    let cid_bytes = cid(data).to_bytes();
    let messages = respond(
        &server,
        &request(vec![want(cid_bytes.clone(), WantType::Have, false)]),
    );
    assert_eq!(messages.len(), 1);
    assert!(messages[0].payload.is_empty());
    assert_eq!(
        messages[0].block_presences,
        vec![presence(cid_bytes, BlockPresenceType::Have)]
    );
}
#[test]
fn missing_block_gives_dont_have_only_when_requested() {
    let server = BitswapServer::new(MemorySource::default());
    let missing = cid(b"missing").to_bytes();
    for want_type in [WantType::Block, WantType::Have] {
        let silent = request(vec![want(missing.clone(), want_type, false)]);
        assert!(respond(&server, &silent).is_empty());
        let explicit = request(vec![want(missing.clone(), want_type, true)]);
        let messages = respond(&server, &explicit);
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].block_presences,
            vec![presence(missing.clone(), BlockPresenceType::DontHave)]
        );
    }
}
#[test]
fn cancel_entries_and_empty_requests_produce_no_response() {
    let data = b"present";
    let server = BitswapServer::new(MemorySource::with(&[data]));
    let mut cancel = want(cid(data).to_bytes(), WantType::Block, true);
    cancel.cancel = true;
    assert!(respond(&server, &request(vec![cancel])).is_empty());
    assert!(respond(&server, &request(vec![])).is_empty());
    assert!(server.plan(&BitswapMessage::default()).unwrap().is_empty());
}
#[test]
fn priority_does_not_change_the_response() {
    let (a, b) = (b"block a".as_slice(), b"block b".as_slice());
    let server = BitswapServer::new(MemorySource::with(&[a, b]));
    let mut low = want(cid(a).to_bytes(), WantType::Block, false);
    let mut high = want(cid(b).to_bytes(), WantType::Block, false);
    low.priority = 1;
    high.priority = 1000;
    let messages = respond(&server, &request(vec![low, high]));
    let received: Vec<&[u8]> = messages[0]
        .payload
        .iter()
        .map(|block| block.data.as_slice())
        .collect();
    assert_eq!(received, vec![a, b]);
}
#[test]
fn priority_and_cancel_survive_encoding() {
    let mut entry = want(cid(b"x").to_bytes(), WantType::Block, false);
    entry.priority = 7;
    entry.cancel = true;
    let message = request(vec![entry.clone()]);
    let decoded = BitswapMessage::from_frame(&message.to_frame()).unwrap();
    assert_eq!(decoded.wantlist.unwrap().entries, vec![entry]);
}
#[test]
fn undecodable_cid_is_answered_with_dont_have() {
    let server = BitswapServer::new(MemorySource::default());
    // CIDv0 (sam multihash) nie jest obsługiwany przez ten węzeł.
    let mut cid_v0 = vec![0x12, 0x20];
    cid_v0.extend_from_slice(&[7; 32]);
    let req = request(vec![
        want(cid_v0.clone(), WantType::Block, true),
        want(vec![0xff, 0xff], WantType::Have, false),
    ]);
    let messages = respond(&server, &req);
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].block_presences,
        vec![presence(cid_v0, BlockPresenceType::DontHave)]
    );
}
#[test]
fn mixed_wantlist_is_answered_in_one_message() {
    let (a, b) = (b"block a".as_slice(), b"block b".as_slice());
    let server = BitswapServer::new(MemorySource::with(&[a, b]));
    let missing = cid(b"missing").to_bytes();
    let req = request(vec![
        want(cid(a).to_bytes(), WantType::Block, false),
        want(cid(b).to_bytes(), WantType::Have, false),
        want(missing.clone(), WantType::Block, true),
    ]);
    let messages = respond(&server, &req);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].payload.len(), 1);
    assert_eq!(messages[0].payload[0].data, a);
    assert_eq!(
        messages[0].block_presences,
        vec![
            presence(cid(b).to_bytes(), BlockPresenceType::Have),
            presence(missing, BlockPresenceType::DontHave),
        ]
    );
}
#[test]
fn wantlist_is_truncated_to_entry_limit() {
    let blocks: Vec<Vec<u8>> = (0..MAX_WANTLIST_ENTRIES + 5)
        .map(|index| format!("block {index}").into_bytes())
        .collect();
    let refs: Vec<&[u8]> = blocks.iter().map(Vec::as_slice).collect();
    let server = BitswapServer::new(MemorySource::with(&refs));
    let entries = blocks
        .iter()
        .map(|data| want(cid(data).to_bytes(), WantType::Have, false))
        .collect();
    let plan = server.plan(&request(entries)).unwrap();
    assert_eq!(plan.presences.len(), MAX_WANTLIST_ENTRIES);
    assert_eq!(plan.ignored_entries, 5);
}
#[test]
fn large_response_is_split_into_messages_within_limit() {
    let blocks: Vec<Vec<u8>> = (0u8..3).map(|index| vec![index; 400]).collect();
    let refs: Vec<&[u8]> = blocks.iter().map(Vec::as_slice).collect();
    let server = BitswapServer::with_limits(MemorySource::with(&refs), limits(1000, usize::MAX));
    let entries = blocks
        .iter()
        .map(|data| want(cid(data).to_bytes(), WantType::Block, false))
        .collect();
    let messages = respond(&server, &request(entries));
    assert_eq!(messages.len(), 2);
    assert!(messages.iter().all(|message| message.encoded_len() <= 1000));
    let received: Vec<Vec<u8>> = messages
        .iter()
        .flat_map(|message| message.payload.iter().map(|block| block.data.clone()))
        .collect();
    assert_eq!(received, blocks);
}
#[test]
fn response_stops_at_total_byte_limit() {
    let blocks: Vec<Vec<u8>> = (0u8..3).map(|index| vec![index; 400]).collect();
    let refs: Vec<&[u8]> = blocks.iter().map(Vec::as_slice).collect();
    let server = BitswapServer::with_limits(MemorySource::with(&refs), limits(4096, 500));
    let entries = blocks
        .iter()
        .map(|data| want(cid(data).to_bytes(), WantType::Block, false))
        .collect();
    let mut plan = server.plan(&request(entries)).unwrap();
    let message = server.next_message(&mut plan).unwrap().unwrap();
    assert_eq!(message.payload.len(), 1);
    assert!(server.next_message(&mut plan).unwrap().is_none());
    assert_eq!(plan.sent_block_bytes, 400);
    assert_eq!(plan.sent_blocks, vec![cid(&blocks[0])]);
    assert_eq!(plan.dropped_blocks, 2);
}
#[test]
fn block_larger_than_message_limit_is_not_sent() {
    let data = vec![9u8; 400];
    let server = BitswapServer::with_limits(MemorySource::with(&[&data]), limits(100, usize::MAX));
    let cid_bytes = cid(&data).to_bytes();
    let messages = respond(
        &server,
        &request(vec![want(cid_bytes.clone(), WantType::Block, true)]),
    );
    assert_eq!(messages.len(), 1);
    assert!(messages[0].payload.is_empty());
    assert_eq!(
        messages[0].block_presences,
        vec![presence(cid_bytes, BlockPresenceType::DontHave)]
    );
}
#[test]
fn block_removed_after_planning_becomes_dont_have() {
    let server = BitswapServer::new(VanishingSource);
    let cid_bytes = cid(b"gone").to_bytes();
    let messages = respond(
        &server,
        &request(vec![want(cid_bytes.clone(), WantType::Block, true)]),
    );
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].block_presences,
        vec![presence(cid_bytes, BlockPresenceType::DontHave)]
    );
    let silent = request(vec![want(cid(b"gone").to_bytes(), WantType::Block, false)]);
    assert!(respond(&server, &silent).is_empty());
}
#[test]
fn source_failure_is_reported_not_hidden() {
    let server = BitswapServer::new(MemorySource {
        fail: true,
        ..MemorySource::default()
    });
    let req = request(vec![want(cid(b"x").to_bytes(), WantType::Block, true)]);
    assert!(server.plan(&req).is_err());
}
#[test]
fn provided_block_source_serves_only_provided_blocks() {
    let directory = tempfile::tempdir().unwrap();
    let store = PersistentBlockstore::open_in(directory.path()).unwrap();
    let provided = store.put(b"provided block").unwrap();
    let private = store.put(b"stored but not provided").unwrap();
    let announced_without_data = cid(b"announced, no bytes");
    store.add_provide(&provided).unwrap();
    store.add_provide(&announced_without_data).unwrap();
    drop(store);

    let source = ProvidedBlockSource::new(directory.path());
    let cids = [provided.clone(), private.clone(), announced_without_data];
    assert_eq!(source.has_blocks(&cids).unwrap(), vec![true, false, false]);
    assert_eq!(
        source.get_block(&provided).unwrap(),
        Some(b"provided block".to_vec())
    );
    assert_eq!(source.get_block(&private).unwrap(), None);

    let store = PersistentBlockstore::open_in(directory.path()).unwrap();
    store.remove_provide(&provided).unwrap();
    drop(store);
    assert_eq!(
        source.has_blocks(std::slice::from_ref(&provided)).unwrap(),
        vec![false]
    );
    assert_eq!(source.get_block(&provided).unwrap(), None);
}
#[test]
fn provided_block_source_treats_unsupported_cid_as_absent() {
    let directory = tempfile::tempdir().unwrap();
    let source = ProvidedBlockSource::new(directory.path());
    let mut unsupported = cid(b"x");
    unsupported.multihash.code = 0x1e;
    assert_eq!(
        source
            .has_blocks(std::slice::from_ref(&unsupported))
            .unwrap(),
        vec![false]
    );
    assert_eq!(source.get_block(&unsupported).unwrap(), None);
}
