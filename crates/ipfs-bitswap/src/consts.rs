pub const BITSWAP_PROTOCOL: &str = "/ipfs/bitswap/1.2.0";

pub const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_WANTLIST_ENTRIES: usize = 100;
pub const MAX_RESPONSE_BYTES: usize = 512 * 1024 * 1024;

pub const STREAM_WRITE_TIMEOUT_SECS: u64 = 30;
/// How long the client waits for a block or DontHave from one provider.
pub const FETCH_RESPONSE_TIMEOUT_SECS: u64 = 30;
pub const WANT_PRIORITY: i32 = 1;
/// How many blocks the client asks for in one wantlist (the server accepts up to `MAX_WANTLIST_ENTRIES`).
pub const FETCH_WINDOW: usize = 64;
