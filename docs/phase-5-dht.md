# Phase 5 — Kademlia DHT: provider discovery (M5, Issue #7)

## Cel
Węzeł ogłasza, że posiada blok (`start_providing`), a inny węzeł znajduje dostawcę po CID (`get_providers`). DHT zwraca **PeerId dostawców**, a nie dane. Pobranie bloku to zadanie Bitswap (M6/M7).

## Kluczowe fakty (z researchu)
1. **Klucz rekordu = multihash, nie CID.** `ProviderKey` trzyma bajty `varint(code) || varint(len) || digest`. To identyczny format jak klucz Blockstore (faza 3) i ten sam klucz dla `raw` i `dag-pb`.
2. **Dwa DHT w Kubo:** WAN `/ipfs/kad/1.0.0` (tylko peery z publicznymi adresami) i LAN `/ipfs/lan/kad/1.0.0` (adresy prywatne). Demo na localhost/LAN musi używać **LAN** (`KadProtocol::Lan`, domyślne). WAN przełączamy dla publicznej sieci.
3. **Tryb Server jest obowiązkowy.** rust-libp2p domyślnie działa w trybie Client, dopóki nie potwierdzi adresu zewnętrznego. Client nie odpowiada na `GET_PROVIDERS`/`ADD_PROVIDER`, więc Kubo nie widziałby Rusta. `DhtConfig::force_server_mode = true`.
4. **Identify → `kad.add_address`.** Peer, który sam do nas dzwoni, nie trafia do tablicy routingu automatycznie. `register_identified_peer` dodaje go, jeśli ogłasza nasz protokół Kademlia.
5. **Adres zewnętrzny.** Bez `swarm.add_external_address(..)` ADD_PROVIDER nie niesie adresów dostawcy. Demo dodaje adresy nasłuchu; w produkcji użyj adresu z Identify/AutoNAT.
6. **Rekordy dostawcy są w pamięci** (`MemoryStore`) i giną po restarcie. Po starcie wywołaj `DhtTracker::announce_all` dla wszystkich bloków z Blockstore. Okresową republikację robi kad (`provider_publication_interval`).
7. `get_providers` zwraca też rekordy lokalne, więc `DhtTracker` odfiltrowuje własny PeerId.

## Nowe pliki
| Plik | Rola |
|---|---|
| `crates/ipfs-network/src/dht.rs` | `DhtConfig`, `KadProtocol`, `ProviderKey`, `DhtTracker`, `DhtEvent`, `register_identified_peer`, testy jednostkowe |
| `crates/ipfs-network/src/bin/ipfs-dht-node.rs` | samodzielny węzeł demo (stdin: `provide`/`find`/`stop`/`bootstrap`/`peers`) |
| `scripts/testing/kubo-dht-interop.sh` | scenariusze A/B/C Rust↔Kubo |
| `docs/phase-5-dht.md` | ten dokument |

## Zmiany w istniejących plikach
Nie widziałem kodu `ipfs-network` (GitHub blokuje przeglądanie katalogów), więc poniższe zmiany wynikają z opisu architektury w `phase-4-network.md`. Dopasuj nazwy do swojego kodu.

**`crates/ipfs-network/Cargo.toml`**
```toml
libp2p = { version = "0.56.0", features = ["tokio","tcp","noise","yamux","identify","ping","kad","macros","ed25519"] }
unsigned-varint = "0.8"   # ta sama wersja co w ipfs-cid
data-encoding = "2"
futures = "0.3"
thiserror = "2"           # jeśli jeszcze nie ma
tokio = { version = "1", features = ["macros","rt-multi-thread","io-std","io-util","time"] }
```

**`src/lib.rs`**
```rust
pub mod dht;
```

**`src/config.rs`** – dodaj pole `pub dht: crate::dht::DhtConfig` do `NetworkConfig` (`Default` zadziała).

**`src/error.rs`**
```rust
#[error(transparent)]
Dht(#[from] crate::dht::DhtError),
```

**`src/behaviour.rs`**
```rust
use crate::dht::Kad;

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    pub kad: Kad,                       // NOWE
}
// w konstruktorze:
// kad: crate::dht::build_kademlia(local_peer_id, &config.dht),
```

**`src/node.rs`**
```rust
// przed pętlą zdarzeń:
let mut dht = crate::dht::DhtTracker::new(local_peer_id);

// 1) przy każdym adresie z listy bootstrap, obok dial:
swarm.behaviour_mut().kad.add_address(&peer_id, addr_bez_p2p);

// 2) nowe ramiona w match na SwarmEvent:
SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. })) => {
    // ...istniejące logowanie IDENTIFY...
    crate::dht::register_identified_peer(
        &mut swarm.behaviour_mut().kad, config.dht.protocol, peer_id, &info);
}
SwarmEvent::Behaviour(BehaviourEvent::Kad(ev)) => {
    for e in dht.on_event(ev) { tracing::info!(?e, "DHT"); }
}
SwarmEvent::NewListenAddr { address, .. } => {
    // ...istniejące...
    swarm.add_external_address(address);   // tylko demo LAN/localhost
}
```
Do API węzła dodaj komendy (kanał `mpsc`) `Provide(ProviderKey)` i `FindProviders(ProviderKey)`, które wywołują `dht.start_providing(..)` / `dht.get_providers(..)` na `swarm.behaviour_mut().kad`. Po M2 zasil `announce_all` kluczami z `Blockstore::list_blocks()`.

**`.github/workflows`** (opcjonalnie) – dodaj `cargo test -p ipfs-network`; skrypt Kubo wymaga demona, więc uruchamiaj go lokalnie.

## Uruchomienie
```bash
cargo test -p ipfs-network
./scripts/testing/kubo-dht-interop.sh                      # localhost
HOST_IP=192.168.1.20 ./scripts/testing/kubo-dht-interop.sh # LAN
```
Ręcznie (dwa terminale):
```bash
cargo run -p ipfs-network --bin ipfs-dht-node -- --key ./tmp/a.key.pb --listen /ip4/127.0.0.1/tcp/4101
cargo run -p ipfs-network --bin ipfs-dht-node -- --key ./tmp/b.key.pb --listen /ip4/127.0.0.1/tcp/4102 \
  --bootstrap /ip4/127.0.0.1/tcp/4101/p2p/<PEER_A>
# w A:  provide bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku
# w B:  find    bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku
```

## Status testów (uczciwie)
Kod **nie został skompilowany ani uruchomiony** w środowisku, w którym powstał (brak Rusta i sieci). Wyniki testów jednostkowych i skryptu interop wpisz dopiero po prawdziwym uruchomieniu.

## Ryzyka do sprawdzenia przy pierwszym uruchomieniu
- Szczegóły API `libp2p 0.56` (np. `kad::Config::new`, pola `kad::Event`) mogą wymagać drobnych poprawek względem Twojego `Cargo.lock`.
- Czy Kubo dodaje peera na loopbacku do LAN DHT. Jeśli skrypt A/B zawiedzie na 127.0.0.1, powtórz z `HOST_IP` ustawionym na adres LAN. Sprawdź też `ipfs id` pod kątem `/ipfs/lan/kad/1.0.0`.
- `ipfs init --profile=test`, `ipfs routing findprovs/provide` – flagi zależą od wersji Kubo (README podaje v0.43.1+).
- W scenariuszu B Kubo musi mieć Rusta w tablicy routingu, żeby wysłać mu ADD_PROVIDER; jeśli nie, `find` w Rust i tak powinien znaleźć rekord przez GET_PROVIDERS do Kubo.

## Ograniczenia
- `MemoryStore` – brak trwałości rekordów (patrz pkt 6).
- Brak NAT traversal i relay (poza zakresem).
- Provider record nie zawiera uprawnień ani treści medycznych; to tylko `PeerId` + adresy. Metadane (kto ma jaki CID) są jawne w DHT, zob. phase-1.1 §10.5.
