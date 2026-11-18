//! Dzielenie pliku na kawałki. Rabin jest portem chunkera Kubo
//! (`whyrusleeping/chunker` + `boxo/chunker`), więc daje te same granice i te same CID.
use crate::consts::{
    DEFAULT_CHUNK_BYTES, FIXED_CHUNK_EXTENSIONS, RABIN_AVG_BYTES, RABIN_MAX_BYTES, RABIN_MIN_BYTES,
    RABIN_POLYNOMIAL, RABIN_WINDOW_BYTES,
};
use crate::error::UnixfsError;
use crate::models::{Chunker, Chunks, RabinTables};
use ipfs_store::consts::MAX_BLOCK_BYTES;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;

impl Chunker {
    pub fn fixed() -> Self {
        Self::Fixed {
            size: DEFAULT_CHUNK_BYTES,
        }
    }

    pub fn rabin() -> Self {
        Self::Rabin {
            min: RABIN_MIN_BYTES,
            avg: RABIN_AVG_BYTES,
            max: RABIN_MAX_BYTES,
        }
    }

    /// Audio i wideo: stałe kawałki; wszystko inne (tekst, obrazy, dokumenty): Rabin.
    pub fn for_path(path: &Path) -> Self {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        match extension {
            Some(extension) if FIXED_CHUNK_EXTENSIONS.contains(&extension.as_str()) => {
                Self::fixed()
            }
            _ => Self::rabin(),
        }
    }

    /// Nazwa w notacji Kubo (`ipfs add --chunker=...`).
    pub fn kubo_name(&self) -> String {
        match self {
            Self::Fixed { size } => format!("size-{size}"),
            Self::Rabin { min, avg, max } => format!("rabin-{min}-{avg}-{max}"),
        }
    }

    pub fn split<R: Read>(self, reader: R) -> Result<Chunks<R>, UnixfsError> {
        let rabin = match self {
            Self::Fixed { size } if size == 0 || size > MAX_BLOCK_BYTES => {
                return Err(UnixfsError::InvalidChunker(
                    "size must be between 1 and the block limit",
                ));
            }
            Self::Fixed { .. } => None,
            // Te same warunki co w Kubo; kawałek musi też zmieścić się w bloku.
            Self::Rabin { min, avg, max }
                if min < RABIN_WINDOW_BYTES
                    || min >= avg
                    || avg >= max
                    || max > MAX_BLOCK_BYTES =>
            {
                return Err(UnixfsError::InvalidChunker(
                    "need 16 <= min < avg < max <= block limit",
                ));
            }
            Self::Rabin { avg, .. } => Some(Box::new(RabinTables::new(avg))),
        };
        Ok(Chunks {
            reader: BufReader::new(reader),
            chunker: self,
            rabin,
        })
    }
}

impl<R: Read> Iterator for Chunks<R> {
    type Item = io::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = match (self.chunker, &self.rabin) {
            (Chunker::Rabin { min, max, .. }, Some(tables)) => {
                tables.next_chunk(&mut self.reader, min, max)
            }
            // Drugi wzorzec jest nieosiągalny: `split` zawsze buduje tablice dla Rabina.
            (Chunker::Fixed { size }, _) | (Chunker::Rabin { max: size, .. }, None) => {
                let mut chunk = Vec::new();
                (&mut self.reader)
                    .take(size as u64)
                    .read_to_end(&mut chunk)
                    .map(|_| chunk)
            }
        };
        match chunk {
            Ok(chunk) if chunk.is_empty() => None,
            other => Some(other),
        }
    }
}

/// Reszta z dzielenia wielomianów nad GF(2).
fn polynomial_mod(mut value: u64, divisor: u64) -> u64 {
    let degree = |polynomial: u64| 63 - polynomial.leading_zeros() as i32;
    while value != 0 && degree(value) >= degree(divisor) {
        value ^= divisor << (degree(value) - degree(divisor));
    }
    value
}

impl RabinTables {
    fn new(average: usize) -> Self {
        let degree = 63 - RABIN_POLYNOMIAL.leading_zeros();
        let append = |hash: u64, byte: u64| polynomial_mod((hash << 8) | byte, RABIN_POLYNOMIAL);
        let (mut out, mut modulo) = ([0u64; 256], [0u64; 256]);
        for byte in 0..256u64 {
            // Wkład bajtu, który właśnie wypada z okna.
            out[byte as usize] =
                (1..RABIN_WINDOW_BYTES).fold(append(0, byte), |hash, _| append(hash, 0));
            modulo[byte as usize] =
                polynomial_mod(byte << degree, RABIN_POLYNOMIAL) | (byte << degree);
        }
        Self {
            out,
            modulo,
            shift: degree - 8,
            // Kubo obcina średnią w dół do potęgi dwójki.
            mask: (1u64 << average.ilog2()) - 1,
        }
    }

    fn roll(
        &self,
        digest: &mut u64,
        window: &mut [u8; RABIN_WINDOW_BYTES],
        at: &mut usize,
        byte: u8,
    ) {
        let leaving = std::mem::replace(&mut window[*at], byte);
        *at = (*at + 1) % RABIN_WINDOW_BYTES;
        *digest ^= self.out[leaving as usize];
        let index = (*digest >> self.shift) as usize;
        *digest = ((*digest << 8) | u64::from(byte)) ^ self.modulo[index];
    }

    /// Kolejny kawałek; pusty na końcu danych. Stan haszu zaczyna się od nowa w każdym
    /// kawałku, więc granice zależą tylko od treści od ostatniego cięcia.
    fn next_chunk(&self, reader: &mut impl BufRead, min: usize, max: usize) -> io::Result<Vec<u8>> {
        let (mut digest, mut window, mut at) = (0u64, [0u8; RABIN_WINDOW_BYTES], 0usize);
        self.roll(&mut digest, &mut window, &mut at, 1);
        // Pierwsze bajty nie mogą dać cięcia, więc nie są haszowane; okno zdąży się wypełnić.
        let unhashed = min - RABIN_WINDOW_BYTES;
        let mut chunk = Vec::new();
        loop {
            let buffer = reader.fill_buf()?;
            if buffer.is_empty() {
                return Ok(chunk);
            }
            let mut used = 0;
            let mut cut = false;
            for &byte in buffer {
                used += 1;
                let length = chunk.len() + used;
                if length <= unhashed {
                    continue;
                }
                self.roll(&mut digest, &mut window, &mut at, byte);
                if length >= min && (digest & self.mask == 0 || length >= max) {
                    cut = true;
                    break;
                }
            }
            chunk.extend_from_slice(&buffer[..used]);
            reader.consume(used);
            if cut {
                return Ok(chunk);
            }
        }
    }
}
