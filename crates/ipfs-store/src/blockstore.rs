use crate::consts::{DEFAULT_DB_PATH, LEVELDB_DIR_NAME};
use crate::datastore::Datastore;
use crate::error::StoreError;
use crate::models::{Blockstore, KnownPeer, LevelDbDatastore, Namespace, RemoteProvider};
use crate::utils::{
    decode_known_peer, decode_remote_provider, encode_known_peer, encode_remote_provider,
    get_multihash_key, get_namespaced_multihash_key, get_peer_key, get_remote_provider_key,
    validate_block_size, validate_cid, validate_known_peer, validate_remote_provider,
};
use ipfs_cid::consts::{CODEC_RAW, SHA2_256};
use ipfs_cid::models::Cid;
use std::path::Path;

impl<D: Datastore> Blockstore<D> {
    pub fn new_blockstore(datastore: D) -> Self {
        Self { datastore }
    }

    /// Tworzy CIDv1 raw/SHA2-256 i zapisuje blok.
    pub fn put(&self, bytes: &[u8]) -> Result<Cid, StoreError> {
        self.put_with_codec(bytes, CODEC_RAW, SHA2_256)
    }

    pub fn put_with_codec(
        &self,
        bytes: &[u8],
        codec: u64,
        hash_code: u64,
    ) -> Result<Cid, StoreError> {
        validate_block_size(bytes)?;
        let cid = Cid::generate_from_data(bytes, codec, hash_code)?;
        self.put_block(&cid, bytes)?;
        Ok(cid)
    }

    /// Zapisuje blok po zweryfikowaniu CID. Deduplikuje po multihash.
    pub fn put_block(&self, cid: &Cid, bytes: &[u8]) -> Result<(), StoreError> {
        validate_block_size(bytes)?;
        validate_cid(cid)?;
        if !cid.verify_block(bytes) {
            return Err(StoreError::IntegrityMismatch);
        }

        let key = get_multihash_key(&cid.multihash);
        if self.datastore.put_if_absent(&key, bytes)? {
            eprintln!(
                "[ipfs-store] STORED cid={} bytes={}",
                cid.to_string_base32(),
                bytes.len()
            );
            return Ok(());
        }

        match self.datastore.get(&key)? {
            Some(existing) if existing == bytes => {
                eprintln!("[ipfs-store] ALREADY_EXISTS cid={}", cid.to_string_base32());
                Ok(())
            }
            Some(_) => Err(StoreError::CorruptStoredBlock),
            None => Err(StoreError::ConcurrentModification),
        }
    }

    /// Odczyt z kontrolą integralności; None oznacza brak bloku.
    pub fn get_block(&self, cid: &Cid) -> Result<Option<Vec<u8>>, StoreError> {
        validate_cid(cid)?;
        let key = get_multihash_key(&cid.multihash);
        match self.datastore.get(&key)? {
            None => Ok(None),
            Some(bytes) => {
                validate_block_size(&bytes)?;
                if !cid.verify_block(&bytes) {
                    return Err(StoreError::IntegrityMismatch);
                }
                Ok(Some(bytes))
            }
        }
    }

    /// Sprawdza tylko obecność klucza, bez ponownego haszowania bloku.
    pub fn has(&self, cid: &Cid) -> Result<bool, StoreError> {
        validate_cid(cid)?;
        self.datastore.has(&get_multihash_key(&cid.multihash))
    }

    /// Zapisuje lub nadpisuje znanego peera (przestrzeń nazw `Peers`).
    pub fn save_peer(&self, peer: &KnownPeer) -> Result<(), StoreError> {
        validate_known_peer(peer)?;
        self.datastore
            .put(&get_peer_key(&peer.peer_id), &encode_known_peer(peer))
    }

    /// Wszystkie zapamiętane peery, posortowane po PeerId.
    pub fn load_peers(&self) -> Result<Vec<KnownPeer>, StoreError> {
        let prefix = Namespace::Peers.prefix();
        let mut peers = Vec::new();
        for (key, value) in self.datastore.entries_with_prefix(prefix)? {
            let peer_id = std::str::from_utf8(&key[prefix.len()..])
                .map_err(|_| StoreError::CorruptRecord("klucz peera nie jest UTF-8".into()))?;
            peers.push(decode_known_peer(peer_id, &value)?);
        }
        Ok(peers)
    }

    pub fn remove_peer(&self, peer_id: &str) -> Result<(), StoreError> {
        self.datastore.delete(&get_peer_key(peer_id))
    }

    pub fn prune_peers(&self, seen_after_utc: u64) -> Result<(Vec<KnownPeer>, usize), StoreError> {
        let (fresh, stale): (Vec<_>, Vec<_>) = self
            .load_peers()?
            .into_iter()
            .partition(|peer| peer.last_seen_utc >= seen_after_utc);
        for peer in &stale {
            self.remove_peer(&peer.peer_id)?;
        }
        Ok((fresh, stale.len()))
    }

    /// Zapamiętuje, że węzeł ogłasza ten CID w DHT (przestrzeń nazw `Provides`).
    pub fn add_provide(&self, cid: &Cid) -> Result<(), StoreError> {
        validate_cid(cid)?;
        let key = get_namespaced_multihash_key(Namespace::Provides, &cid.multihash);
        self.datastore.put(&key, &cid.to_bytes())
    }

    pub fn remove_provide(&self, cid: &Cid) -> Result<(), StoreError> {
        validate_cid(cid)?;
        let key = get_namespaced_multihash_key(Namespace::Provides, &cid.multihash);
        self.datastore.delete(&key)
    }

    /// Czy węzeł ogłasza ten CID; tylko takie bloki wolno wydawać innym peerom.
    pub fn has_provide(&self, cid: &Cid) -> Result<bool, StoreError> {
        validate_cid(cid)?;
        self.datastore.has(&get_namespaced_multihash_key(
            Namespace::Provides,
            &cid.multihash,
        ))
    }

    /// CID, które należy ponownie ogłosić w DHT po restarcie węzła.
    pub fn list_provides(&self) -> Result<Vec<Cid>, StoreError> {
        self.datastore
            .entries_with_prefix(Namespace::Provides.prefix())?
            .iter()
            .map(|(_, value)| Ok(Cid::decode_from_bytes(value)?))
            .collect()
    }

    /// Zapisuje rekord „peer ma blok” otrzymany z DHT (przestrzeń `RemoteProviders`).
    /// Ponowny zapis tej samej pary multihash + peer odświeża termin ważności.
    pub fn save_remote_provider(&self, provider: &RemoteProvider) -> Result<(), StoreError> {
        validate_remote_provider(provider)?;
        let key = get_remote_provider_key(&provider.multihash, &provider.peer_id);
        self.datastore.put(&key, &encode_remote_provider(provider))
    }

    /// Ważne rekordy providerów; `now_utc` to sekundy od epoki Unix (UTC).
    pub fn load_remote_providers(&self, now_utc: u64) -> Result<Vec<RemoteProvider>, StoreError> {
        Ok(self.scan_remote_providers(now_utc, false)?.0)
    }

    /// Jednym przejściem usuwa przeterminowane rekordy i zwraca (ważne, liczba usuniętych).
    pub fn prune_remote_providers(
        &self,
        now_utc: u64,
    ) -> Result<(Vec<RemoteProvider>, usize), StoreError> {
        self.scan_remote_providers(now_utc, true)
    }

    fn scan_remote_providers(
        &self,
        now_utc: u64,
        delete_expired: bool,
    ) -> Result<(Vec<RemoteProvider>, usize), StoreError> {
        let mut providers = Vec::new();
        let mut removed = 0;
        for (key, value) in self
            .datastore
            .entries_with_prefix(Namespace::RemoteProviders.prefix())?
        {
            let provider = decode_remote_provider(&key, &value)?;
            if provider.expires_at_utc > now_utc {
                providers.push(provider);
            } else if delete_expired {
                self.datastore.delete(&key)?;
                removed += 1;
            }
        }
        Ok((providers, removed))
    }

    pub fn datastore(&self) -> &D {
        &self.datastore
    }
}

impl Blockstore<LevelDbDatastore> {
    pub fn open_default() -> Result<Self, StoreError> {
        let datastore = LevelDbDatastore::open(DEFAULT_DB_PATH)?;
        Ok(Self::new_blockstore(datastore))
    }

    /// Dla testów integracyjnych. EHR-CLI używa open_default()
    pub fn open_in(base_directory: &Path) -> Result<Self, StoreError> {
        let datastore = LevelDbDatastore::open(base_directory.join(LEVELDB_DIR_NAME))?;
        Ok(Self::new_blockstore(datastore))
    }
}
