# 04 — Etapy (M0–M11), status i znane problemy repo

Tablica GitHub Projects (stan 2026-11-12, zrzut od autora):

| Kolumna | Issue |
|---|---|
| **Done** | #1 [M0] Cargo workspace, VS Code, CI · #2 [M1] CIDv1 + testy Kubo · #3 [M2] Blockstore · #4 [M3] Rust↔Kubo libp2p · #5 Bootstrap, Identify, reconnect · #6 [M5] Kademlia routing · #7 [M5] DHT provider discovery + test Kubo |
| **In Progress** | #8 [M6] minimalny Bitswap server · #9 [M7] minimalny Bitswap client · #10 [M8] raw files i podstawowy UnixFS |
| **Backlog** | #11 [M9] modele EHR (pacjent, dokument) · #12 [M10] członkostwo i replikacja na 3 węzłach · #13 [M11] walidacja końcowa, dokumentacja, release |
| Poza tablicą | **Warstwa bezpieczeństwa GPG + UCAN** (ostatni etap; wymagana w DoD; warto założyć issue) |

## Status szczegółowy

| Etap | Kod | Dokument | Testy (potwierdzone?) |
|---|---|---|---|
| M0 | workspace, hooki, rustfmt | `docs/phase-1-planning.md` | — |
| M1 CID | `crates/ipfs-cid` | `docs/phase-2-CID.md` | unit + integracja + interop Kubo: **100% zgodności** (raw, json; mocki XML/TTL/BMP/SVG/TIFF/TXT) |
| M2 Blockstore | `crates/ipfs-store` | `docs/phase-3-blockstore.md` | unit (MemoryDatastore) + integration-store; status uruchomień do potwierdzenia |
| M3 libp2p | `ipfs-network` | `docs/phase-4-network.md` (stary opis z 0.56 i `src/bin`) | Rust↔Kubo historycznie PASS |
| M4 Bootstrap | `ipfs-network` + CLI | `docs/phase-4-bootstrap-identify-ping-reconnect.md` (aktualny) | **PASS (potwierdzone przez autora 2026-11-12):** 2 komputery (Fedora + WSL2), `integration-network` 4/4, `interop-network` Rust↔Kubo |
| M5 DHT | `dht.rs`, `node.rs`, CLI | `docs/phase-5-dht.md` (zaktualizowany 12.11) | **PASS (potwierdzone przez autora 2026-11-12):** `dht-unit-test`, `interop-dht` / `scripts/testing/test-kubo-dht-interop.sh` (scenariusze A, B, C) |
| M6 Bitswap server | pusty crate | — | — |
| M7 Bitswap client | pusty crate | — | — |
| M8 UnixFS | pusty crate | `docs/phase-8-unixfs.md` (1 linia, stub) | — |
| M9–M11, Security | — | `docs/phase-1.1-planning-role-models.md` (analiza UCAN/GPG) | — |

## Następny krok: M6/M7 — Bitswap (notatki startowe, do weryfikacji w specyfikacji)

Źródło prawdy: https://specs.ipfs.tech/bitswap-protocol/ oraz kod Kubo/boxo. **rust-libp2p 0.57 nie ma gotowego Bitswap.**

- Protokoły: `/ipfs/bitswap/1.2.0` (cel), także 1.1.0, 1.0.0 i `/ipfs/bitswap` dla zgodności.
- Wiadomość: protobuf z prefiksem długości (unsigned varint). Pola: `wantlist { entries[ {block: CID bytes, priority, cancel, wantType: Block|Have, sendDontHave} ], full }`, `payload[ {prefix: CID prefix (version, codec, mh type, mh len), data} ]`, `blockPresences[ {cid, type: Have|DontHave} ]`, `pendingBytes`.
- Model strumieni: wiadomości wysyłane jednokierunkowo, a odpowiedź idzie osobną wiadomością/strumieniem, a nie w trybie request/response. Do rozważenia: crate `libp2p-stream` lub własny `ConnectionHandler` jako kolejne pole `NetworkBehaviour`. **Decyzja OPEN.** Zgodnie z regułą reużycia: dodać do istniejącego `NetworkBehaviour` i `NetworkNode`, bez drugiego Swarm.
- **Server (M6):** po otrzymaniu want-list sprawdź `PersistentBlockstore` → `Have`/`Block` lub `DontHave` (jeśli `sendDontHave`). Test: Kubo `ipfs block get <CID>` po `ipfs swarm connect` do Rusta (lub przez DHT).
- **Client (M7):** `find <CID>` (DHT) → połączenie z dostawcą → `want-have`/`want-block` → odbiór → **`Cid::verify_block`** → `Blockstore::put_block` → cancel. Timeouty, retry, uszkodzony blok, dostawca offline.
- Prefiks CID w payload pozwala odtworzyć pełny CID z danych (`Cid::new_cid(codec, Multihash::compute(...))`).
- Potrzebne zależności: `prost` (+ `prost-build` lub ręczne struktury), istniejące `unsigned-varint`.
- Nowe komendy CLI w działającym węźle (propozycja): `fetch <CID>`, ewentualnie `wantlist`.

## Stan repo — ustalenia autora (2026-11-12)

Elementy, które mogą wyglądać na usterki, są **zamierzone**. Nie zgłaszaj ich jako problemów i nie „naprawiaj” bez prośby autora: śledzona binarka `ehr-cli` w katalogu głównym, brak `.github/workflows`, zahardkodowany `DEFAULT_BOOTSTRAP_MULTIADDR` (Fedora `192.168.1.125:4101`), `docs/phase-4-network.md` jako historyczny opis M3, stub `docs/phase-8-unixfs.md`.
