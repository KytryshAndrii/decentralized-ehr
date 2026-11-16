/// Domyślny rozmiar kawałka Kubo (`size-262144`).
pub const DEFAULT_CHUNK_BYTES: usize = 256 * 1024;

/// Parametry Rabina dla dokumentów EHR (`--chunker=rabin-16384-65536-262144` w Kubo).
/// Domyślne parametry Kubo (87381-262144-393216) tną co ~256 KiB i w ok. 1/3 przypadków
/// dochodzą do maksimum, czyli do cięcia zależnego od pozycji; kilkumegabajtowy dokument
/// po dopisaniu jednej linii nie współdzieli wtedy z poprzednią wersją żadnego bloku.
/// Mniejsza średnia i szeroki zakres min..max dają cięcia zależne od treści w ~98% kawałków.
pub const RABIN_MIN_BYTES: usize = 16 * 1024;
pub const RABIN_AVG_BYTES: usize = 64 * 1024;
pub const RABIN_MAX_BYTES: usize = DEFAULT_CHUNK_BYTES;

/// Wielomian Rabina używany przez Kubo (`IpfsRabinPoly`, stopień 53).
pub const RABIN_POLYNOMIAL: u64 = 17_437_180_132_763_653;
pub const RABIN_WINDOW_BYTES: usize = 16;

/// Maksymalna liczba linków w węźle pliku (`UnixFSFileMaxLinks` w Kubo).
pub const MAX_FILE_LINKS: usize = 174;

/// Pliki audio i wideo: stałe kawałki. Strumienie skompresowane nie mają powtarzalnych
/// fragmentów, więc Rabin nie daje deduplikacji, a kosztuje czas procesora.
pub const FIXED_CHUNK_EXTENSIONS: &[&str] = &[
    "aac", "avi", "flac", "m4a", "m4v", "mkv", "mov", "mp3", "mp4", "mpeg", "mpg", "oga", "ogg",
    "ogv", "opus", "wav", "webm", "wma", "wmv",
];
