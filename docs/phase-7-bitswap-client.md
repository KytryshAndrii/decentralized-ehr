# Phase 7 — Minimalny klient Bitswap (M7, Issue #9)

> **Stan dokumentacji:** 15.11.2026. Testy jednostkowe, integracyjne (3 węzły Rust) oraz interoperacyjności Rust ↔ Kubo 0.43.1 zostały uruchomione i **zakończyły się powodzeniem** (szczegóły w §6).

## 1. Cel i zakres

- Wysyłanie zapytań (Want-Block) do dostawców.
- Odbiór i weryfikacja bloków.
- Integracja pobierania z DHT (M5) i Blockstore (M2).
- **Definition of Done:** Rust pobiera i weryfikuje blok z Kubo.

Źródła: <https://specs.ipfs.tech/bitswap-protocol/>, <https://docs.ipfs.tech/concepts/bitswap/>.

## 2. Decyzje

| Zagadnienie | Decyzja dla M7 |
|---|---|
| Wersja protokołu | Tylko `/ipfs/bitswap/1.2.0`, jak w M6. |
| Odkrywanie | Dostawców wskazuje DHT (`get_providers`). Rozgłaszanie Want-Have do wszystkich połączonych peerów i sesje z dokumentacji Bitswap są pominięte; specyfikacja ich nie wymaga. |
| Żądanie | Jedna wiadomość z jednym wpisem: `wantType = Block`, `sendDontHave = true`, `priority = 1`, `full = false`. |
| Dostawcy | Odpytywani po kolei; pierwszy, który wyda poprawny blok, kończy pobieranie. `DontHave`, timeout (30 s) albo błąd strumienia przełącza na następnego. |
| Cancel | Wysyłany do dostawcy, który odpowiedział `DontHave` albo nie odpowiedział w czasie (Kubo pamięta want do anulowania). Po odebraniu bloku nie jest potrzebny: dostawca sam usuwa wysłany want. |
| Odbiór | Odpowiedź przychodzi strumieniem otwartym przez dostawcę. Protokół ma jeden odbiornik strumieni (`BitswapServer::start`), który przekazuje `payload` i `blockPresences` klientowi. |
| Weryfikacja | CID jest wyliczany z prefiksu i skrótu **odebranych bajtów** (`utils::block_cid`) i dopiero po nim szukane jest żądanie `(peer, CID)`. Blok uszkodzony, niezamówiony albo od innego peera niż zapytany nie pasuje do żadnego żądania (`BITSWAP_BLOCK_REJECTED`). `Blockstore::put_block` weryfikuje CID drugi raz przed zapisem. |
| Zapis | Pobrany blok trafia do `/blocks/`. **Nie jest automatycznie ogłaszany**; do tego służy `provide <CID>`. |
| Limity | Wiadomość ≤ 4 MiB, blok ≤ 2 MiB (`ipfs-store`). |

## 3. Przebieg `fetch <CID>`

1. CLI: jeśli blok jest lokalnie → `BITSWAP_ALREADY_LOCAL`, koniec.
2. `NetworkCommand::Fetch` → `get_providers` w DHT (`BITSWAP_FETCH_REQUESTED`).
3. Po zakończeniu zapytania (`DHT_FIND_FINISHED`) lista dostawców trafia do `BitswapClient::fetch`, które działa w tle (pętla zdarzeń węzła nie jest blokowana).
4. Dla kolejnego dostawcy: otwarcie strumienia (libp2p sam zestawia połączenie), Want-Block, oczekiwanie na blok albo `DontHave`.
5. Blok zgodny z CID → `put_block` → `BITSWAP_BLOCK_RECEIVED <peer> <CID> bytes=N`.
6. Brak dostawców albo żaden nie wydał bloku → `BITSWAP_FETCH_FAILED <CID>: <powód>`.

## 4. Kod

| Miejsce | Zmiana |
|---|---|
| `ipfs-bitswap/src/client.rs` | `BitswapClient::{new, fetch}`, `deliver` (routing odpowiedzi) |
| `ipfs-bitswap/src/utils.rs` | `block_cid(prefix, data)` |
| `ipfs-bitswap/src/service.rs` | `BitswapServer::start(behaviour, &client)` przekazuje odpowiedzi klientowi |
| `ipfs-network` | `NetworkCommand::Fetch`, `NetworkNode::fetch_block`, `pending_fetches` |
| `ehr-cli` | komenda `fetch <CID>` w działającym węźle |
| `ipfs-network/src/dht.rs` | `DHT_MAX_PROVIDED_KEYS = 65 536` zamiast domyślnych 1024 |

## 5. Ograniczenia

- Jeden blok na żądanie; pliki wieloblokowe (DAG) to M8.
- Tylko CIDv1 z SHA2-256 / SHA2-512 (zakres `ipfs-cid`).
- Brak ponawiania: po `BITSWAP_FETCH_FAILED` komendę trzeba powtórzyć.
- Jeśli baza LevelDB jest w chwili zapisu otwarta przez inny proces (`ehr-cli get`), zapis pobranego bloku kończy się błędem i `fetch` trzeba powtórzyć.
- Odrzucenie uszkodzonego bloku jest sprawdzone testem jednostkowym `block_cid`; nie ma testu sieciowego ze złośliwym dostawcą.

## 6. Testy

| Warstwa | Test | Wynik |
|---|---|---|
| Jednostkowe | `cargo test -p ipfs-bitswap` (`received_block_is_matched_to_cid_by_its_own_hash`) | PASS |
| Integracyjne | `integration-bitswap`: `node_fetches_verified_block_from_provider_found_in_dht` (hub DHT, dostawca, pobierający; blok 1,5 MB; `ALREADY_LOCAL`; brak dostawcy) | PASS |
| Interop Kubo 0.43.1 | `scripts/testing/test-kubo-bitswap-interop.sh`, scenariusze E (mały blok z Kubo), F (1 MiB z Kubo), G (CID bez dostawcy) | PASS |
