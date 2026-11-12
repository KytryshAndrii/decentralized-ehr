# 03 — Rejestr decyzji architektonicznych

Status: **ACCEPTED** (obowiązuje), **SUPERSEDED** (zastąpiona, zostawiona dla historii pracy), **PROPOSED** (propozycja, niezatwierdzona), **OPEN** (do decyzji).
Nowe decyzje dopisuj na końcu z datą. Nie usuwaj starych, bo ewolucja decyzji to materiał do pracy dyplomowej.

## A. Decyzje bazowe (docs/phase-1-planning.md)

| ID | Decyzja | Status |
|---|---|---|
| AD-01 | Rust jako jedyny język; ten sam program na wszystkich węzłach, inna konfiguracja ról | ACCEPTED |
| AD-02 | Własna minimalna implementacja IPFS; Kubo tylko jako referencja testowa | ACCEPTED |
| AD-03 | rust-libp2p (TCP, Noise, Yamux, Identify, Ping, kad) | ACCEPTED |
| AD-04 | 3 węzły: A = placówka, B = pacjent 1, C = pacjent 2 | ACCEPTED |
| AD-05 | Dynamiczne dołączanie i odłączanie, trwały PeerId, co najmniej jeden bootstrap | ACCEPTED |
| AD-06 | Kademlia DHT obowiązkowa (provide/find providers) | ACCEPTED |
| AD-07 | Własny CIDv1 (raw + sha2-256, varint, multihash, multibase base32) | ACCEPTED, zrobione |
| AD-08 | Własny lokalny Blockstore (put/get/has/delete/list) | ACCEPTED; delete i list jeszcze nie zrobione |
| AD-09 | Minimalny Bitswap zgodny ze specyfikacją (Want-Have, Want-Block, Have, DontHave, Block) | ACCEPTED, M6/M7 |
| AD-10 | Podstawowy UnixFS: raw → odczyt dag-pb → pliki wieloblokowe, jeden deterministyczny chunker | ACCEPTED, M8 |
| AD-11 | Pacjent zarządza własnym rejestrem dokumentów i CID; manifesty | ACCEPTED, M9 |
| AD-12 | Replikacja jawna (pobranie + zachowanie bloków) | ACCEPTED, M10 |
| AD-13 | **Bez IPNS i bez blockchaina**; zmiana dokumentu = nowy CID, historia w rejestrze EHR | ACCEPTED |
| AD-14 | „Brak rozbudowanego modułu cyberbezpieczeństwa”, bez własnych algorytmów krypto | **ZMODYFIKOWANA przez D-15**: bezpieczeństwo jest minimalne, ale obowiązkowe |
| AD-15 | CLI jako podstawowy interfejs (web opcjonalnie) | ACCEPTED |
| AD-16 | Mobile tylko jako kierunek rozwoju | ACCEPTED |

## B. Decyzje z realizacji etapów

| ID | Data | Decyzja | Status |
|---|---|---|---|
| D-01 | M1 | `VarintDecodeError` bez wartości (`map_err(|_| ...)`); dekoder odrzuca trailing bytes i wersję ≠ 1 | ACCEPTED |
| D-02 | M1 | Testy integracyjne i interop w osobnym pakiecie `test-runner`, jednostkowe w crate | ACCEPTED |
| D-03 | M1 | Interop Kubo sekwencyjnie (`--test-threads=1`), bo `repo.lock` | ACCEPTED |
| D-04 | M2 | Backend Datastore = **LevelDB (`rusty-leveldb` 4.0.2)** zamiast własnego FilesystemDatastore/FlatFS | ACCEPTED (FilesystemDatastore SUPERSEDED) |
| D-05 | M2 | Klucz bazy = `/blocks/` + **multihash** (bez kodeka), więc deduplikacja raw/dag-pb | ACCEPTED |
| D-06 | M2 | Weryfikacja integralności przy put **i** get; `has` tylko obecność | ACCEPTED |
| D-07 | M2 | Max blok 2 MiB; 256 KiB to rozmiar chunku UnixFS, nie minimum bloku | ACCEPTED |
| D-08 | M2 | `ehr-cli` zna tylko `PersistentBlockstore::open_default()`; generyczny `Blockstore<D>` zostaje dla testów (MemoryDatastore) | ACCEPTED |
| D-09 | M4 | `libp2p 0.57.1` (wcześniejsze notatki o 0.56 są nieaktualne) | ACCEPTED |
| D-10 | M4 | Stała tożsamość `./tmp/node-key.pb`, auto-tworzona; **brak flagi do ścieżki klucza**; klucz zależny od CWD | ACCEPTED |
| D-11 | M4 | Trzy tryby bootstrap (wzajemnie wykluczające się): listen-only, `--default-bootstrap`, `--bootstrap <addr>`; stary pozycyjny multiaddr jako alias | ACCEPTED (rozszerzone w M5, zob. D-14) |
| D-12 | M4 | `DialOpts::peer_id(..).addresses(..)` + pomijanie dial, gdy `swarm.is_connected(peer)` | ACCEPTED, w kodzie |
| D-13 | M4 | Retry 2→4→8→16→30 s, tick 1 s, ping 15 s, idle 90 s; reconnect tylko przy `num_established == 0` | ACCEPTED |
| D-14 | M5 | `node start` bez argumentów = domyślny listen + **domyślny bootstrap** + Client; `""` = jawnie bez bootstrapu | ACCEPTED |
| D-16 | M5 | Kademlia dołączona do istniejącego `NetworkBehaviour` (jeden Swarm), `MemoryStore`, timeout zapytań 30 s | ACCEPTED |
| D-17 | M5 | Klucz rekordu providera = **multihash** (nie pełny CID); `provider_key()` korzysta z istniejącego `Cid` | ACCEPTED |
| D-18 | M5 | **DHT Client domyślnie (pacjent), Server przez `--dht-server` (placówka)**; ustawiane przy starcie | ACCEPTED |
| D-19 | M5 | Adresy z Identify trafiają do Kademlii tylko, gdy peer ogłasza protokół DHT; po tym `kad.bootstrap()` | ACCEPTED |
| D-20 | M5 | Brak AutoNAT, mDNS i automatycznej zmiany Client→Server w M5 | ACCEPTED (warunki awansu w `05-...` §5 jako wymaganie na przyszłość) |
| D-21 | M5 | Rekordy providerów tylko w pamięci, więc po restarcie trzeba ponowić `provide` (brak `announce_all`) | ACCEPTED jako ograniczenie |

### D-22 — Wybór sieci DHT: LAN vs WAN ⚠️ KONFLIKT, wymaga potwierdzenia autora

- **Wersja 1 (rozmowa z ChatGPT, ok. 23–24.10):** publiczny IPFS WAN DHT (Amino, `/ipfs/kad/1.0.0`) domyślnie + LAN (`/ipfs/lan/kad/1.0.0`) przez flagę developerską `--dev-lan-dht`. Status: **SUPERSEDED?**
- **Wersja 2 (`docs/phase-5-dht.md`, stan 11.11.2026, oraz KOD):** **wyłącznie LAN DHT**, WAN nie jest implementowany ani wybierany flagą. Uzasadnienie: prototyp za jednym routerem, prywatne IP; sieci zdalne przez routowany VPN L3 z tym samym LAN DHT. Kod: `consts::LAN_DHT_PROTOCOL`, CLI drukuje `[DHT] LAN Client|Server`.
- **Obowiązuje wersja 2 (kod + najnowszy dokument).** Wariant WAN zostaje w pracy jako alternatywa i kierunek rozwoju (wymaga publicznie osiągalnych adresów; ujawnia metadane w publicznym DHT). Jeśli autor zechce wrócić do WAN, potrzebny jest konfigurowalny `DhtProtocol` w `dht.rs` + flaga CLI. **Zapytaj autora, zanim zmienisz.**

### D-15 — Warstwa bezpieczeństwa: GPG + UCAN (obie)

- `docs/phase-1.1-planning-role-models.md` rekomendował **IPFS + GPG + lokalna logika ról**, UCAN tylko jako opcjonalne rozszerzenie.
- **Aktualna decyzja autora (2026-11-12):** w DoD pracy wchodzi **minimalne szyfrowanie + autoryzacja UCAN + GPG** jako dodatkowa warstwa na gotowym węźle IPFS, realizowana w ostatnim etapie. Status: **ACCEPTED**. Szczegóły i otwarte decyzje D01–D05 są w `06-security-ucan-gpg.md`.

### D-23 — Role pacjent / placówka

- Ten sam binarny `ehr-cli`. Różnice: (1) tryb DHT (Client/Server), (2) uprawnienia (UCAN + GPG) w ostatnim etapie. Do tego czasu dokumentacja sieciowa używa nazw Host A/B. Status: **ACCEPTED**.

### D-24 — Bitswap ≠ DHT Server/Client

- Zadania M6 „Bitswap server” i M7 „Bitswap client” dotyczą **wymiany bloków** (odpowiadanie na want-listy / wysyłanie want-list), a nie trybu Kademlii. Implementacja Bitswap była celowo odłożona do zakończenia M5. Status: **ACCEPTED**; teraz, po M5, przechodzimy do M6/M7.

### D-25 — Cykl życia danych (PROPOSED, do M9/M10)

Pinning jako odpowiedzialność za przechowywanie, GC dla niepinowanych, usuwanie = tombstone + unpin + zniszczenie klucza (crypto-erasure), `RecordId ≠ CID`, wersjonowanie, retencja konfigurowalna (PL: zwykle 20 lat). Szczegóły w `07-data-lifecycle.md`. **Nie implementowane; zatwierdzić zakres z autorem.**
