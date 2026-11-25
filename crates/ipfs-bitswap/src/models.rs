use futures::channel::oneshot;
use ipfs_cid::models::Cid;
use libp2p::PeerId;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub type BitswapBehaviour = libp2p_stream::Behaviour;
pub type BitswapControl = libp2p_stream::Control;
pub type BitswapIncomingStreams = libp2p_stream::IncomingStreams;

#[derive(Clone, PartialEq, prost::Message)]
pub struct BitswapMessage {
    #[prost(message, optional, tag = "1")]
    pub wantlist: Option<Wantlist>,
    #[prost(bytes = "vec", repeated, tag = "2")]
    pub blocks: Vec<Vec<u8>>,
    #[prost(message, repeated, tag = "3")]
    pub payload: Vec<Block>,
    #[prost(message, repeated, tag = "4")]
    pub block_presences: Vec<BlockPresence>,
    #[prost(int32, tag = "5")]
    pub pending_bytes: i32,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Wantlist {
    #[prost(message, repeated, tag = "1")]
    pub entries: Vec<WantlistEntry>,
    #[prost(bool, tag = "2")]
    pub full: bool,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct WantlistEntry {
    #[prost(bytes = "vec", tag = "1")]
    pub block: Vec<u8>,
    /// Unused by the server (it is stateless); the M7 client sets the request order.
    #[prost(int32, tag = "2")]
    pub priority: i32,
    /// The server skips such entries without an answer; there is no wantlist to remove them from.
    #[prost(bool, tag = "3")]
    pub cancel: bool,
    #[prost(enumeration = "WantType", tag = "4")]
    pub want_type: i32,
    #[prost(bool, tag = "5")]
    pub send_dont_have: bool,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct Block {
    /// CID version, codec, multihash code and length (without the digest).
    #[prost(bytes = "vec", tag = "1")]
    pub prefix: Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub data: Vec<u8>,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct BlockPresence {
    #[prost(bytes = "vec", tag = "1")]
    pub cid: Vec<u8>,
    #[prost(enumeration = "BlockPresenceType", tag = "2")]
    pub r#type: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum WantType {
    Block = 0,
    Have = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, prost::Enumeration)]
#[repr(i32)]
pub enum BlockPresenceType {
    Have = 0,
    DontHave = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BitswapLimits {
    pub max_message_bytes: usize,
    pub max_wantlist_entries: usize,
    pub max_response_bytes: usize,
}

/// Block planned to be sent in answer to a Want-Block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlannedBlock {
    pub cid: Cid,
    pub send_dont_have: bool,
}

#[derive(Debug, Default)]
pub struct ResponsePlan {
    pub presences: Vec<BlockPresence>,
    pub blocks: VecDeque<PlannedBlock>,
    /// CIDs of the blocks already placed in messages.
    pub sent_blocks: Vec<Cid>,
    pub sent_block_bytes: usize,
    /// Entries over the wantlist limit, skipped without an answer.
    pub ignored_entries: usize,
    /// Blocks skipped after the total answer limit was reached.
    pub dropped_blocks: usize,
    pub(crate) carried: Option<(Cid, Block)>,
}

pub struct BitswapServer<S> {
    pub(crate) source: Arc<S>,
    pub(crate) limits: BitswapLimits,
}

/// Blockstore-backed block source; serves only announced blocks (`provide`).
/// The database is opened for one call.
#[derive(Clone, Debug)]
pub struct ProvidedBlockSource {
    pub(crate) directory: PathBuf,
}

/// Expected answers: (provider, binary CID) -> block, or `None` for DontHave.
pub(crate) type WantMap = HashMap<(PeerId, Vec<u8>), oneshot::Sender<Option<Vec<u8>>>>;
pub(crate) type PendingWants = Arc<Mutex<WantMap>>;

/// Bitswap client: sends Want-Block and stores the verified block in the Blockstore.
/// Answers arrive on streams received by `BitswapServer`.
#[derive(Clone)]
pub struct BitswapClient {
    pub(crate) control: BitswapControl,
    pub(crate) directory: PathBuf,
    pub(crate) pending: PendingWants,
}
