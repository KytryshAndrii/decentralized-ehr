# AGENT.md — punkt wejścia dla agentów (Claude, ChatGPT, inne)

> **Przeczytaj ten plik jako pierwszy.** Zawiera kontekst projektu, twarde reguły i mapę pozostałych plików `knowledge/`.
> Ostatnia aktualizacja: 2026-11-12 (po M5 — Kademlia DHT, przed M6/M7 — Bitswap).
> Właściciel: Andrii Kytrysh (GitHub `KytryshAndrii`). Repo: https://github.com/KytryshAndrii/decentralized-ehr (lokalnie: `~/ipfs_prototype`, WSL2 Ubuntu).

## 1. Projekt w jednym akapicie

Praca inżynierska: **„Implementacja zdecentralizowanego systemu EHR opartego o protokół IPFS”**. W Rust (Cargo workspace, edition 2024) powstaje **własna, minimalna implementacja wybranych mechanizmów IPFS** (CIDv1, Blockstore, libp2p, Kademlia DHT, Bitswap, UnixFS), sprawdzana na każdym etapie pod kątem **interoperacyjności z Kubo** (implementacja referencyjna). Na gotowym węźle IPFS powstaje warstwa EHR (pacjent, dokument, manifest, rejestr CID), a na samym końcu **warstwa bezpieczeństwa: minimalne szyfrowanie + autoryzacja (GPG/OpenPGP + UCAN)**. Interfejs: `apps/ehr-cli`. Demonstracja: 3 fizyczne komputery (placówka + 2 pacjentów). Praca ma charakter analityczno-projektowy, implementacja jest demonstracyjna, nie produkcyjna, i nie tworzy nowych algorytmów ani protokołów.

## 2. Mapa plików `knowledge/`

| Plik | Treść | Kiedy czytać |
|---|---|---|
| `01-thesis-scope.md` | Temat z USOS 1:1, cel, zadania, literatura, **Definition of Done całej pracy** | Zawsze przy planowaniu |
| `02-architecture-and-code-rules.md` | Struktura crate'ów, publiczne API, **wzorce projektowe i konwencje kodu** | Przed każdą zmianą kodu |
| `03-decisions-log.md` | Rejestr decyzji (AD-xx), także **zastąpionych** | Przed proponowaniem zmian architektury |
| `04-milestones-status.md` | M0–M11, status, tablica GitHub Projects, znane problemy repo | Przy planowaniu kolejnego etapu |
| `05-network-dht.md` | M4 + M5: bootstrap, reconnect, Kademlia, Client/Server, LAN/WAN, XOR | Praca nad siecią, DHT, Bitswap |
| `06-security-ucan-gpg.md` | Ostatni etap: szyfrowanie + autoryzacja, role pacjent/placówka | Projekt warstwy bezpieczeństwa |
| `07-data-lifecycle.md` | Pinning, GC, usuwanie, retencja, wersjonowanie rekordów | M9/M10 i rozdział teoretyczny |
| `08-environment-and-testing.md` | Maszyny, IP, PeerId, porty, Kubo, polecenia testów, pułapki | Uruchamianie i testowanie |
| `09-notes-and-open-questions.md` | Notatki autora, odpowiedzi na pytania, otwarte kwestie | Przed zadaniem pytania autorowi |
| `10-sources.md` | Literatura i źródła (dokumentacja IPFS = źródło prawdy) | Przy pisaniu dokumentacji/pracy |

Dokumentacja etapów dla pracy dyplomowej leży osobno w `docs/phase-*.md`. `knowledge/` to **kontekst roboczy dla agentów**, a nie rozdziały pracy.

## 3. Hierarchia źródeł prawdy (przy konflikcie)

1. **Kod w repo (`master`) + `Cargo.lock`**: tak jest naprawdę.
2. **Najnowsza decyzja autora w rozmowie**, zapisana w `03-decisions-log.md`.
3. `docs/phase-*.md`: najnowszy plik etapu wygrywa ze starszym.
4. Notatki z rozmów z innymi AI (ChatGPT): to tylko propozycje, chyba że autor je zatwierdził.
5. Wiedza o IPFS: **dokumentacja i specyfikacje IPFS/libp2p** (docs.ipfs.tech, specs.ipfs.tech, github.com/libp2p/specs), nie pamięć modelu.

## 4. Twarde reguły pracy (ustalone przez autora)

1. **Reużywaj, nie przepisuj.** Nowa funkcjonalność ma korzystać z istniejących typów i mechanizmów: `ipfs_cid::models::Cid`, `PersistentBlockstore`, ten sam `Swarm`, `NetworkNode`, `NetworkCommand`, `identity::load_or_create`. Nie twórz drugiego parsera CID, drugiego Swarm ani osobnych binarek demo, jeśli da się rozszerzyć istniejące.
2. **Wersje bibliotek z projektu.** `libp2p = 0.57.1` (sprawdź `Cargo.lock`). Nie zmieniaj wersji bez prośby autora. API dobieraj do tej wersji.
3. **Wzorce projektowe są obowiązkowe** (fasada, serwis, trait jako strategia/adapter, komenda, pętla zdarzeń, builder). Szczegóły w `02-...`.
4. **Enkapsulacja crate'ów:** każdy crate wystawia przemyślane metody publiczne, a szczegóły są `pub(crate)` lub prywatne. **Publiczne API ma być realnie używane przez `apps/ehr-cli`**. CLI nie sięga do backendów (np. nie importuje `LevelDbDatastore`).
5. **Interoperacyjność z Kubo testujemy od razu w każdym etapie**, nie na końcu. Każdy komponent ma 3 warstwy testów: jednostkowe (`crates/*/tests/*-unit-test.rs`), integracyjne Rust↔Rust (`test-runner/tests/integration`) i interop z Kubo (`test-runner/tests/interoperability` + `scripts/testing/*.sh`).
6. **Uczciwy status.** Kod napisany ≠ kod skompilowany ≠ test zaliczony. W dokumentacji `PASS` wpisujemy tylko po rzeczywistym uruchomieniu, z hashem commita.
7. **Żadnych prawdziwych danych medycznych.** Tylko syntetyczne (mocki HL7 FHIR, syntetyczne EKG w `test-runner/mocks/`).
8. **Terminologia:** tryb DHT Client/Server ≠ rola biznesowa Pacjent/Placówka. Do etapu autoryzacji w dokumentacji sieci używamy nazw Host A/B, node, peer.
9. **Pacjent i placówka uruchamiają ten sam program.** Różnią się wyłącznie (a) trybem DHT (pacjent: Client domyślnie, placówka: Server przez `--dht-server`) oraz (b) uprawnieniami w ostatnim etapie (UCAN + GPG).
10. Pre-commit (`.githooks/pre-commit`): `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo clippy -D warnings`, `cargo test --workspace`. Kod musi to przejść.
11. Commity w stylu autora: prefiks `#<obszar>` (np. `#network`, `#dht`, `#cid`) + krótki opis. Commituj tylko na prośbę.
12. Komunikacja z autorem po polsku (autor pisze też po angielsku). Gdy prosi o „super short / 10x short”, odpowiadaj bardzo krótko.

## 5. Stan na dziś (skrót, szczegóły w `04-...`)

- **Zrobione i przetestowane (PASS, w tym interop z Kubo):** M0 (workspace), M1 (CIDv1 zgodne z Kubo), M2 (Blockstore + LevelDB), M3 (libp2p Rust↔Kubo), M4 (bootstrap, Identify, Ping, reconnect), M5 (Kademlia LAN DHT, Client/Server, provide/find).
- Stan repo (binarka `ehr-cli` w git, brak workflows, zahardkodowany bootstrap) jest **zamierzony**. Nie zgłaszaj tego jako problemów (zob. `04-...`).
- **W toku (wg tablicy):** M6 Bitswap server, M7 Bitswap client, M8 UnixFS (raw + podstawowy UnixFS).
- **Backlog:** M9 modele EHR, M10 trzy węzły + replikacja, M11 walidacja, dokumentacja, release. Potem warstwa szyfrowania i autoryzacji (UCAN + GPG).
- `crates/ipfs-bitswap`, `crates/ipfs-unixfs` i `crates/ehr-core` to na razie puste szablony (`add()`).
- **M6/M7 „Bitswap server/client” to NIE są tryby DHT server/client.** Bitswap to protokół wymiany bloków (`/ipfs/bitswap/1.2.0`), a DHT tylko odnajduje dostawców.
