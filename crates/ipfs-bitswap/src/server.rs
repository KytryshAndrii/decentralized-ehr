use crate::consts::{MAX_MESSAGE_BYTES, MAX_RESPONSE_BYTES, MAX_WANTLIST_ENTRIES};
use crate::error::BitswapError;
use crate::models::{
    BitswapLimits, BitswapMessage, BitswapServer, Block, BlockPresenceType, PlannedBlock,
    ResponsePlan, WantType,
};
use crate::source::{BlockSource, ReadBlock};
use crate::utils::{block_presence, cid_prefix};
use ipfs_cid::models::Cid;
use prost::Message;
use std::sync::Arc;

impl Default for BitswapLimits {
    fn default() -> Self {
        Self {
            max_message_bytes: MAX_MESSAGE_BYTES,
            max_wantlist_entries: MAX_WANTLIST_ENTRIES,
            max_response_bytes: MAX_RESPONSE_BYTES,
        }
    }
}

impl ResponsePlan {
    pub fn is_empty(&self) -> bool {
        self.presences.is_empty() && self.blocks.is_empty() && self.carried.is_none()
    }
}

impl<S: BlockSource> BitswapServer<S> {
    pub fn new(source: S) -> Self {
        Self::with_limits(source, BitswapLimits::default())
    }

    pub fn with_limits(source: S, limits: BitswapLimits) -> Self {
        Self {
            source: Arc::new(source),
            limits,
        }
    }

    pub fn limits(&self) -> BitswapLimits {
        self.limits
    }

    /// Decides every wantlist entry: Have, DontHave or a block to send.
    /// `cancel` and priority are ignored, because the server keeps no state.
    pub fn plan(&self, request: &BitswapMessage) -> Result<ResponsePlan, BitswapError> {
        let mut plan = ResponsePlan::default();
        let Some(wantlist) = &request.wantlist else {
            return Ok(plan);
        };
        let limit = self.limits.max_wantlist_entries;
        plan.ignored_entries = wantlist.entries.len().saturating_sub(limit);

        let mut wanted = Vec::new();
        for entry in wantlist.entries.iter().take(limit) {
            if entry.cancel {
                continue;
            }
            match Cid::decode_from_bytes(&entry.block) {
                Ok(cid) => wanted.push((cid, entry)),
                Err(_) if entry.send_dont_have => plan
                    .presences
                    .push(block_presence(&entry.block, BlockPresenceType::DontHave)),
                Err(_) => {}
            }
        }
        let cids: Vec<Cid> = wanted.iter().map(|(cid, _)| cid.clone()).collect();
        let available = self.source.has_blocks(&cids)?;

        for ((cid, entry), available) in wanted.into_iter().zip(available) {
            match (available, entry.want_type()) {
                (true, WantType::Have) => plan
                    .presences
                    .push(block_presence(&entry.block, BlockPresenceType::Have)),
                (true, WantType::Block) => plan.blocks.push_back(PlannedBlock {
                    cid,
                    send_dont_have: entry.send_dont_have,
                }),
                (false, _) if entry.send_dont_have => plan
                    .presences
                    .push(block_presence(&entry.block, BlockPresenceType::DontHave)),
                (false, _) => {}
            }
        }
        Ok(plan)
    }

    /// Next answer message, no larger than the message limit; `None` once the whole plan
    /// was sent. The source is opened once per message.
    pub fn next_message(
        &self,
        plan: &mut ResponsePlan,
    ) -> Result<Option<BitswapMessage>, BitswapError> {
        self.source
            .with_reader(|read| self.build_message(plan, read))
    }

    fn build_message(
        &self,
        plan: &mut ResponsePlan,
        read: &ReadBlock<'_>,
    ) -> Result<Option<BitswapMessage>, BitswapError> {
        let mut message = BitswapMessage {
            block_presences: std::mem::take(&mut plan.presences),
            ..BitswapMessage::default()
        };
        while let Some((cid, block, send_dont_have)) = next_block(plan, read)? {
            if plan.sent_block_bytes + block.data.len() > self.limits.max_response_bytes {
                plan.dropped_blocks += 1 + plan.blocks.len();
                plan.blocks.clear();
                break;
            }
            let field_bytes = block.encoded_len();
            let needed = 1 + prost::length_delimiter_len(field_bytes) + field_bytes;
            if message.encoded_len() + needed > self.limits.max_message_bytes {
                if message.is_empty() {
                    // The block alone does not fit in a message, so it cannot be served.
                    plan.dropped_blocks += 1;
                    if send_dont_have {
                        message
                            .block_presences
                            .push(block_presence(&cid.to_bytes(), BlockPresenceType::DontHave));
                    }
                    continue;
                }
                plan.carried = Some((cid, block));
                break;
            }
            plan.sent_block_bytes += block.data.len();
            plan.sent_blocks.push(cid);
            message.payload.push(block);
        }
        // DontHave for blocks that disappeared after the answer was planned.
        message.block_presences.append(&mut plan.presences);
        Ok((!message.is_empty()).then_some(message))
    }
}

/// Next block of the plan; a missing block turns into DontHave.
fn next_block(
    plan: &mut ResponsePlan,
    read: &ReadBlock<'_>,
) -> Result<Option<(Cid, Block, bool)>, BitswapError> {
    if let Some((cid, block)) = plan.carried.take() {
        return Ok(Some((cid, block, false)));
    }
    while let Some(planned) = plan.blocks.pop_front() {
        match read(&planned.cid)? {
            Some(data) => {
                let block = Block {
                    prefix: cid_prefix(&planned.cid),
                    data,
                };
                return Ok(Some((planned.cid, block, planned.send_dont_have)));
            }
            None if planned.send_dont_have => plan.presences.push(block_presence(
                &planned.cid.to_bytes(),
                BlockPresenceType::DontHave,
            )),
            None => {}
        }
    }
    Ok(None)
}
