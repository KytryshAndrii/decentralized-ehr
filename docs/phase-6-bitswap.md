# Phase 6 — Minimalny serwer Bitswap (M6, Issue #8)

> **Stan dokumentacji:** 15.11.2026. Opis odpowiada kodowi w katalogu roboczym (zmiany niezacommitowane). Testy jednostkowe, integracyjne oraz interoperacyjności Rust ↔ Kubo 0.43.1 zostały uruchomione i **zakończyły się powodzeniem**.

## 1. Cel i zakres

- Kodowanie wiadomości Bitswap w protobuf i obsługa strumieni libp2p.
- Obsługa żądań **Want-Have** i **Want-Block**.
- Zwracanie treści bloku z lokalnego Blockstore.
- **Definition of Done:** Kubo pobiera znany blok z węzła Rust przez Bitswap.

**Granica odpowiedzialności M6:** węzeł tylko *odpowiada* na cudze wantlisty. Wysyłanie własnych wantlist (pobieranie bloków od innych) to M7. DHT (M5) nadal odpowiada wyłącznie na pytanie „kto ma blok”.

## 2. Decyzje

| Zagadnienie | Decyzja dla M6 |
|---|---|
| Wersja protokołu | Tylko `/ipfs/bitswap/1.2.0`. Wersje 1.1.0 i 1.0.0 nie są rejestrowane. |
| Podpięcie do libp2p | `libp2p-stream 0.5.0-alpha` (surowe strumienie); zgodny z `libp2p-swarm 0.48.0` z libp2p 0.57.1. Logika protokołu jest własna. Jeden wspólny `Swarm`. |
| Protobuf | `prost` z makrami derive, bez `protoc` i `build.rs`. `prost` był już w drzewie zależności libp2p. |
| Stan | Serwer **bezstanowy**: odpowiada od razu, nie pamięta wantlist; `priority` jest ignorowane, a wpisy z `cancel` są pomijane bez odpowiedzi. Pola są w strukturach protobuf (pełny schemat 1.2.0), bo będzie ich używał klient z M7. |
| Które bloki | Tylko bloki **ogłoszone** (`provide`), czyli obecne w `/provides/` i w `/blocks/`. Blok zapisany przez `put`, ale nieogłoszony, nie jest wydawany. |
| Komu | Każdemu połączonemu peerowi. Autoryzacja (UCAN + GPG) dojdzie w ostatnim etapie. |
| Limity | 100 wpisów w wantliście, 4 MiB na wiadomość (wymóg specyfikacji), 512 MiB łącznie na jedno żądanie. Wartości 100 i 512 MiB są przykładowe (`consts.rs`). |
| Księgowość (ledger) | Brak. To mechanizm wzajemności dla sieci publicznej; tutaj o dostępie zdecyduje UCAN. |
| Dostęp do bazy | LevelDB otwierana na czas jednego wywołania przez publiczne metody `ipfs-store`. |

## 3. Format wiadomości

Każda wiadomość na strumieniu to długość jako unsigned varint i protobuf `Message`:

| Pole (nr) | Znaczenie |
|---|---|
| `wantlist` (1) | `entries` (1) i `full` (2) |
| `wantlist.entries[]` | `block` (1) = binarny CID, `priority` (2), `cancel` (3), `wantType` (4): `Block = 0`, `Have = 1`, `sendDontHave` (5) |
| `blocks` (2) | Pole wersji 1.0.0; nieużywane |
| `payload[]` (3) | `prefix` (1) = wersja CID, kodek, kod i długość multihasha; `data` (2) = bajty bloku |
| `blockPresences[]` (4) | `cid` (1), `type` (2): `Have = 0`, `DontHave = 1` |
| `pendingBytes` (5) | Nieużywane (serwer wysyła 0) |

Odpowiedź jest wysyłana **nowym strumieniem otwartym przez węzeł odpowiadający**, a nie strumieniem, którym przyszło żądanie.

## 4. Reguły odpowiedzi

| Wpis wantlisty | Blok ogłoszony i obecny | Brak bloku albo blok nieogłoszony |
|---|---|---|
| Want-Have | `Have` | `DontHave`, jeśli `sendDontHave`; inaczej brak odpowiedzi |
| Want-Block | blok w `payload` | `DontHave`, jeśli `sendDontHave`; inaczej brak odpowiedzi |
| `cancel = true` | pomijany | pomijany |
| CID nie do zdekodowania (np. CIDv0) | — | `DontHave`, jeśli `sendDontHave` |

Dodatkowo:

- Wpisy ponad limit 100 są pomijane bez odpowiedzi (log `BITSWAP_WANTLIST_TRUNCATED`).
- Odpowiedź jest dzielona na kolejne wiadomości, tak aby żadna nie przekroczyła 4 MiB.
- Po przekroczeniu 512 MiB łącznie pozostałe bloki nie są wysyłane (log `BITSWAP_RESPONSE_TRUNCATED`).
- Blok, który zniknął między sprawdzeniem a wysyłką, zamienia się w `DontHave`.
- Blok jest przed wysłaniem weryfikowany względem CID przez `Blockstore::get_block`; uszkodzony blok nie jest wysyłany.

## 5. Struktura kodu

`crates/ipfs-bitswap/`:

| Plik | Zawartość |
|---|---|
| `consts.rs` | identyfikator protokołu i limity |
| `error.rs` | `BitswapError` |
| `models.rs` | struktury protobuf (`BitswapMessage`, `Wantlist`, `WantlistEntry`, `Block`, `BlockPresence`), `BitswapLimits`, `ResponsePlan`, `BitswapServer`, `ProvidedBlockSource` |
| `codec.rs` | ramka varint + protobuf, `read_message`, `write_message` |
| `source.rs` | trait `BlockSource` (Strategy) i implementacja `ProvidedBlockSource` nad `ipfs-store` |
| `server.rs` | logika bez sieci: `plan()` i `next_message()` |
| `service.rs` | strumienie: `start()`, przyjmowanie żądań i wysyłka odpowiedzi |
| `utils.rs` | prefiks CID, budowanie `BlockPresence` |

Zmiany w pozostałych crate'ach:

- `ipfs-network`: pole `bitswap` w `NetworkBehaviour`, nowy `bitswap.rs` z `start_bitswap()` wywoływanym w `run_inner`. Serwer startuje tylko wtedy, gdy ustawiono `state_directory`.
- `ipfs-store`: `Blockstore::has_provide(cid)` oraz ponawianie otwarcia bazy, gdy plik `LOCK` trzyma inna krótka operacja (`LOCK_RETRY_ATTEMPTS`, `LOCK_RETRY_DELAY_MS`). Ponawianie jest potrzebne, bo pętla sieciowa i zadania Bitswap otwierają bazę z różnych wątków.
- `ehr-cli`: bez nowych komend; opis w `--help`.

## 6. Logi

| Log | Znaczenie |
|---|---|
| `BITSWAP_LISTENING /ipfs/bitswap/1.2.0` | serwer zarejestrował protokół |
| `BITSWAP_DISABLED no state directory` | węzeł bez katalogu stanu nie udostępnia bloków |
| `BITSWAP_REQUEST <peer> entries=N` | przyszła wantlista (także same `cancel`) |
| `BITSWAP_PRESENCE_SENT <peer> have=N dont_have=M` | wysłano Have/DontHave |
| `BITSWAP_BLOCK_SENT <peer> <CID> bytes=N` | wysłano blok |
| `BITSWAP_WANTLIST_TRUNCATED`, `BITSWAP_RESPONSE_TRUNCATED` | zadziałał limit |
| `BITSWAP_FAILED <peer>: <błąd>` | błąd strumienia, dekodowania albo bazy |

## 7. Użycie

Terminal 1 (węzeł Rust):

```bash
./ehr-cli put /abs/path/plik.txt
```

```bash
./ehr-cli node start --bootstrap /ip4/<IP>/tcp/<port>/p2p/<PeerId-Kubo> --dht-server
```

W działającym węźle: `provide <CID>`.

Terminal 2 (Kubo połączone z węzłem Rust):

```bash
ipfs block get <CID>
```

## 8. Testy

| Rodzaj | Plik | Zakres |
|---|---|---|
| Jednostkowe (21) | `crates/ipfs-bitswap/tests/bitswap-unit-test.rs` | bajty protobuf zgodne ze schematem, ramki, limity, Want-Have, Want-Block, DontHave, błędny CID, dzielenie odpowiedzi, limit łączny, `ProvidedBlockSource` |
| Jednostkowe | `crates/ipfs-store/tests/store-unit-test.rs` | `has_provide` |
| Integracyjny | `test-runner/tests/integration/bitswap_integration.rs` | prawdziwy proces `ehr-cli` i minimalny klient testowy libp2p: Have, blok 35 B, blok 1,5 MB, blok nieogłoszony, blok po `stop` |
| Interop z Kubo | `scripts/testing/test-kubo-bitswap-interop.sh`, `test-runner/tests/interoperability/bitswap_interop.rs` | Kubo widzi protokół w Identify, pobiera blok 45 B i 1 MiB (`ipfs block get`, porównanie bajtów), nie dostaje bloku nieogłoszonego |

Uruchomienie:

```bash
cargo test -p ipfs-bitswap
```

```bash
cargo build -p ehr-cli && cargo test -p test-runner --test integration-bitswap --test interop-bitswap -- --nocapture
```

## 9. Ograniczenia

- Brak autoryzacji: każdy połączony peer może pobrać ogłoszony blok (do etapu UCAN + GPG).
- `priority` nic nie zmienia, a `cancel` tylko pomija wpis: serwer nie ma wantlisty, z której mógłby coś usunąć ani kolejki do uporządkowania.
- Brak stanu: jeśli blok zostanie ogłoszony po żądaniu, węzeł go nie dośle; pytający musi zapytać ponownie.
- Tylko CIDv1; CIDv0 dostaje `DontHave`.
- Brak limitu liczby równoległych strumieni i żądań na peera.
- `libp2p-stream` ma oznaczenie `alpha`.
- Wpisy ponad limit wantlisty są pomijane bez `DontHave`.
- Węzeł nie pobiera bloków (M7).
