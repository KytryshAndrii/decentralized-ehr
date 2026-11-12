# 02 — Architektura kodu, publiczne API i reguły pisania kodu

## 1. Workspace

```text
Cargo.toml (resolver 3, edition 2024 w crate'ach)
├── apps/ehr-cli            # JEDYNA aplikacja; warstwa "kontrolera" + serwisy
│   └── src/main.rs, src/services/{mod.rs, ipfs_service.rs}
├── crates/ipfs-cid         # M1 — CIDv1, Multihash (gotowe)
├── crates/ipfs-store       # M2 — Blockstore<D: Datastore>, LevelDB (gotowe)
├── crates/ipfs-network     # M3–M5 — libp2p Swarm, Identify, Ping, reconnect, Kademlia (gotowe)
├── crates/ipfs-bitswap     # M6/M7 — PUSTY szablon
├── crates/ipfs-unixfs      # M8 — PUSTY szablon
├── crates/ehr-core         # M9 — PUSTY szablon (modele EHR)
├── test-runner             # testy czarnoskrzynkowe: integration/ + interoperability/, mocks/
├── scripts/testing/*.sh    # scenariusze z Kubo i wieloma procesami
└── docs/phase-*.md         # dokumentacja etapów do pracy
```

Zależności (kierunek → „używa”):
`ehr-cli → ipfs-network, ipfs-store, ipfs-cid`; `ipfs-network → ipfs-cid`; `ipfs-store → ipfs-cid`.
Plan: `ipfs-bitswap → ipfs-cid, ipfs-store, (libp2p)`; `ipfs-unixfs → ipfs-cid, ipfs-store`; `ehr-core → ipfs-cid, (unixfs)`.

## 2. Wewnętrzny układ plików crate'a (konwencja autora)

Każdy crate ma ten sam podział, a nowe crate'y mają go powielać:

| Plik | Zawartość |
|---|---|
| `lib.rs` | Tylko `pub mod ...;` + `pub use` publicznej fasady |
| `models.rs` | **Wyłącznie definicje struktur/enumów** (pola `pub(crate)`, gdy to stan wewnętrzny) |
| `<nazwa>.rs` (np. `cid.rs`, `blockstore.rs`, `node.rs`, `dht.rs`) | Bloki `impl` dla struktur z `models.rs` |
| `consts.rs` | Wszystkie stałe (kody multicodec, ścieżki, timeouty, protokoły). Żadnych magicznych liczb w logice |
| `error.rs` | Jeden enum błędów crate'a, `thiserror`, komunikaty po polsku lub angielsku |
| `utils.rs` | Funkcje pomocnicze (walidacja, budowa kluczy) |
| `tests/<crate>-unit-test.rs` | Testy jednostkowe **zewnętrzne** (rejestrowane w `[[test]]` w Cargo.toml), nie `#[cfg(test)]` w źródle |

## 3. Publiczne API (stan obecny) i kto go używa

### `ipfs-cid`
- `Cid { version, codec, multihash }`, `Multihash { code, size, digest }`
- `Cid::new_cid`, `generate_from_data(data, codec, hash)`, `to_bytes`, `decode_from_bytes` (odrzuca trailing bytes, tylko v1), `to_string_base32`, `decode_from_string` (dowolny multibase), `verify_block(data)`
- `Multihash::compute(data, code)` (SHA2-256 0x12, SHA2-512 0x13)
- Stałe: `CID_V1`, `CODEC_RAW 0x55`, `CODEC_DAG_PB 0x70`, `CODEC_DAG_CBOR 0x71`, `CODEC_JSON 0x0200`

### `ipfs-store`
- `trait Datastore { get, put, has, put_if_absent }`: abstrakcja key-value.
- `Blockstore<D: Datastore>`: `new_blockstore`, `put` (raw/sha256), `put_with_codec`, `put_block(cid, bytes)` (weryfikacja), `get_block` (weryfikacja przy odczycie), `has` (tylko obecność klucza), `datastore()`.
- `PersistentBlockstore = Blockstore<LevelDbDatastore>`: **fasada dla aplikacji**, `open_default()` (ścieżka `data/ehr-blockstore.leveldb` względna do CWD), `open_in(dir)` (dla testów).
- Klucz LevelDB: `b"/blocks/" + varint(code) + varint(len) + digest` (multihash, bez kodeka, więc deduplikacja między kodekami).
- Limit bloku `MAX_BLOCK_BYTES = 2 MiB`.
- Brak jeszcze: `delete`, `list_blocks`, pinning, GC (potrzebne do announce-all po restarcie oraz w M10).

### `ipfs-network`
- `NetworkConfig` (listen, identity_path, bootstrap, czasy retry, ping, idle, `dht_mode`) + `Default`.
- `BootstrapPeer::parse("/ip4/.../tcp/.../p2p/<PeerId>")`, `BootstrapPeer::default_bootstrap()`.
- `NetworkNode::new_network_node(config)`, `local_peer_id`, `listen`, `add_bootstrap`, `start_providing(&Cid)`, `get_providers(&Cid)`, `stop_providing(&Cid)`, `run()`, `run_with_commands(mpsc::UnboundedReceiver<NetworkCommand>)`.
- `NetworkCommand { StartProviding, GetProviders, StopProviding, Bootstrap, ListPeers, Quit }`: **wzorzec Command**, sterowanie działającym węzłem z CLI.
- `DhtMode { Client (default), Server }`, `dht::build_kademlia`, `dht::provider_key(&Cid)` (multihash bez wersji i kodeka).
- `identity::load_or_create(path)` (Ed25519, protobuf, `0o600`, `create_new`, bez nadpisywania), `peer_id_from_keypair`.
- `NetworkBehaviour { identify, ping, kad }` (derive `NetworkBehaviour`), `NetworkEvent { Identify, Ping, Kad }`.
- Wewnętrzne (`pub(crate)`): `RetryState`, pola `NetworkNode` (swarm, retries, pending_provider_queries, ...).

### `apps/ehr-cli`
- `main.rs`: parsowanie argumentów **ręcznie** (bez clap), podkomendy `node identity|start`, `cid|put|has|get`, wątek stdin → `mpsc` → `NetworkNode`.
- `services::IpfsService`: **warstwa serwisu** (controller → service → lib). `new()` otwiera `PersistentBlockstore`, `create_cid`, `put_file`, `has_block`, `get_block`, `export_file` (bez nadpisywania). Ścieżki muszą być absolutne.
- Zasada z commitu `#cid`: *„controller layer should call as few lib/service methods as possible”*. Logika ma żyć w crate'ach lub serwisie, a `main.rs` tylko mapuje argumenty i wypisuje wynik.

## 4. Wzorce projektowe w użyciu (kontynuuj je)

| Wzorzec | Gdzie | Uwaga dla nowych etapów |
|---|---|---|
| **Facade** | `PersistentBlockstore::open_default()`; `NetworkNode` ukrywa Swarm | Bitswap/UnixFS powinny mieć analogiczną fasadę dla CLI |
| **Strategy / Adapter (trait + generyk)** | `Blockstore<D: Datastore>`; `MemoryDatastore` w testach, `LevelDbDatastore` w produkcji | Rust nie ma dziedziczenia: kompozycja + trait zamiast OOP |
| **Service layer** | `apps/ehr-cli/src/services/ipfs_service.rs` | Nowe operacje CLI dodawaj jako serwisy (np. `NetworkService`, `EhrService`) |
| **Command** | `NetworkCommand` przez `tokio::mpsc` | Bitswap `want <CID>` / `fetch <CID>` jako nowe warianty komend, a nie drugi proces |
| **Event loop / Observer** | `NetworkNode::run_inner` (`tokio::select!`: tick, swarm event, command, Ctrl+C); `on_swarm_event`, `on_dht_event` | Bitswap jako kolejne pole w `NetworkBehaviour` + ramię `NetworkEvent::Bitswap` |
| **Builder** | `SwarmBuilder`, `build_kademlia` | |
| **Composite behaviour** | `#[derive(NetworkBehaviour)]` | Jeden Swarm, wiele protokołów |
| **Newtype / type alias** | `PersistentBlockstore`, `KademliaBehaviour` | |

Odpowiedź autorowi na pytanie „jak projektować modularnie bez OOP”: **traits (interfejsy) + generyki (polimorfizm statyczny) lub `dyn Trait` (dynamiczny) + kompozycja struktur + moduły z kontrolą widoczności (`pub`, `pub(crate)`)**. Dziedziczenie implementacji zastępujemy domyślnymi metodami traitów i delegacją.

## 5. Konwencje kodu

- Rust edition 2024 (let-chains `if let ... && ...` są używane), `rustfmt.toml`: max_width 100, 4 spacje.
- Błędy: `thiserror`, w aplikacji `Box<dyn Error>` (`CliResult`, `ServiceResult`). Przy błędach zewnętrznych bez wartości używaj `.map_err(|_| ...)`.
- Logi to na razie `println!`/`eprintln!` z **TAGAMI WIELKIMI LITERAMI** do łatwego grep w testach i skryptach: `LOCAL_PEER_ID`, `LISTEN`, `CONNECTED`, `DISCONNECTED`, `IDENTIFY`, `PING`, `DIAL_FAILED`, `DHT_ROUTING_UPDATED`, `DHT_BOOTSTRAP_STARTED`, `DHT_PROVIDE_REQUESTED`, `DHT_PROVIDING_STARTED`, `DHT_PROVIDER`, `DHT_FIND_FINISHED count=N`, `[BLOCKSTORE] PUT CID=`, `[ipfs-store] STORED`, `[ipfs-cid] CREATED`. **Skrypty testowe grepują te tagi, więc nie zmieniaj ich bez aktualizacji skryptów.** Dla Bitswap używaj analogicznych tagów: `BITSWAP_WANT`, `BITSWAP_BLOCK_RECEIVED`, `BITSWAP_SENT`, `BITSWAP_DONT_HAVE`.
- Komentarze i dokumentacja mieszane PL/EN. Dokumentacja pracy jest po polsku.
- Brak `unwrap()` w kodzie biblioteki, z wyjątkiem stałych (`expect("static address")`).
- Pliki prywatne (`tmp/`, `data/`) są w `.gitignore`. Nie commitujemy kluczy ani bazy LevelDB.

## 6. Testy: 3 warstwy

| Warstwa | Lokalizacja | Uruchomienie |
|---|---|---|
| Jednostkowe | `crates/<c>/tests/<c>-unit-test.rs` (`cid-unit-test`, `store-unit-test`, `network-unit-test`, `dht-unit-test`) | `cargo test -p <crate>` |
| Integracyjne (czarna skrzynka, procesy Rust) | `test-runner/tests/integration/*` (`integration-cid`, `integration-store`, `integration-network`) | `cargo build -p ehr-cli && cargo test -p test-runner --test integration-network -- --test-threads=1` |
| Interop z Kubo | `test-runner/tests/interoperability/*` (`interop-cid`, `interop-network`, `interop-dht`) + `scripts/testing/*.sh` | `--test-threads=1` (blokada `repo.lock` Kubo) |

Mocki: `test-runner/mocks/` (HL7 FHIR JSON/XML/TTL, syntetyczne EKG BMP/SVG/TIFF, lorem-ipsum).
Wszystko naraz: `./scripts/testing/run-rust-tests.sh` (fmt, check, clippy, test).

## 7. Jak dodać nowy etap (szablon dla M6+)

1. Implementacja w odpowiednim crate (`ipfs-bitswap`, ...) z układem plików z §2.
2. Integracja z istniejącym `NetworkBehaviour` / `NetworkNode` (jeden Swarm), bez nowych binarek.
3. Nowe komendy w `NetworkCommand` + parser stdin w `ehr-cli` (albo nowy serwis).
4. Testy jednostkowe w `crates/<c>/tests/`, integracyjne w `test-runner`, interop z Kubo w skrypcie + wrapper Rust.
5. Dokument `docs/phase-N-<nazwa>.md` w strukturze z `docs/phase-1-planning.md` §6.
6. Aktualizacja `knowledge/04-milestones-status.md` i, przy nowych decyzjach, `03-decisions-log.md`.
