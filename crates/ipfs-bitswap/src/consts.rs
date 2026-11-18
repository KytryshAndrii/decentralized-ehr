pub const BITSWAP_PROTOCOL: &str = "/ipfs/bitswap/1.2.0";

pub const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_WANTLIST_ENTRIES: usize = 100;
pub const MAX_RESPONSE_BYTES: usize = 512 * 1024 * 1024;

pub const STREAM_WRITE_TIMEOUT_SECS: u64 = 30;
/// Jak długo klient czeka na blok albo DontHave od jednego dostawcy.
pub const FETCH_RESPONSE_TIMEOUT_SECS: u64 = 30;
pub const WANT_PRIORITY: i32 = 1;
/// Ile bloków klient zamawia jedną wantlistą (serwer przyjmuje do `MAX_WANTLIST_ENTRIES`).
pub const FETCH_WINDOW: usize = 64;
