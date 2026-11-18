# Phase 8 — Pliki raw i podstawowy UnixFS (M8, Issue #10)

> **Stan dokumentacji:** 16.11.2026. Testy jednostkowe, integracyjne (procesy `ehr-cli`) oraz interoperacyjności Rust ↔ Kubo 0.43.1 zostały uruchomione i **zakończyły się powodzeniem** (§8).
>
> **Zmiana w M9 (18.11.2026): chunker Rabina usunięty z kodu.** Wszystkie pliki są dzielone stałym chunkerem `size-262144`, bo dokumenty EHR są szyfrogramami (uzasadnienie: `docs/security-ucan-gpg-integration.md` §8, `docs/phase-9-ehr-delivery-plan.md` krok R). Usunięto `Chunker::Rabin`, `Chunker::for_path`, tablice i stałe Rabina oraz testy CID Rabina. Opis Rabina w §2, §4, §5 i §10 zostaje jako historia decyzji. Rust nadal **czyta** pliki Kubo zrobione Rabinem (scenariusz C skryptu interop, PASS 18.11.2026).

## 1. Cel i zakres

- Import i eksport małych plików raw (jeden blok).
- Obsługa dag-pb: budowa DAG pliku i rekonstrukcja pliku z DAG.
- Wymiana plików z Kubo w obie strony.
- **Definition of Done:** raw działa w obie strony; zakres zgodności UnixFS jest udokumentowany (§5).

Źródła: <https://specs.ipfs.tech/unixfs/>, <https://ipld.io/specs/codecs/dag-pb/spec/>, <https://docs.ipfs.tech/concepts/file-systems/>, kod chunkera Kubo (`ipfs/boxo/chunker`, `whyrusleeping/chunker`).

## 2. Decyzje

| Zagadnienie | Decyzja | Uzasadnienie |
|---|---|---|
| Kodek węzłów | **dag-pb** (`0x70`) + liście **raw** (`0x55`) | UnixFS v1 jest zdefiniowany wyłącznie na dag-pb. `ipfs cat`, `ipfs get` i bramki HTTP nie potrafią odczytać pliku z dag-cbor ani z kodeka JSON. |
| dag-cbor | **nie dla plików** | Zob. §3. |
| Układ DAG | **balanced**, do 174 linków na węzeł | Wymóg autora i domyślny układ Kubo; wszystkie liście na tej samej głębokości, więc dostęp do dowolnego miejsca pliku ma stały koszt. Trickle nie jest tworzony. Budowa i matematyka podziału: §10. |
| Liście | raw (`--raw-leaves`) | Liść to same bajty pliku, bez opakowania protobuf. Ten sam fragment danych daje ten sam blok niezależnie od pliku, w którym występuje, a plik jednoblokowy ma ten sam CID co `put`. |
| Chunker dla audio i wideo | stały, 256 KiB (`size-262144`) | Wymóg autora. Strumienie skompresowane nie mają powtarzalnych fragmentów; cięcie zależne od treści kosztowałoby czas bez zysku. |
| Chunker dla pozostałych plików | Rabin `16384-65536-262144` | Wymóg autora co do metody; parametry zmienione względem domyślnych Kubo, zob. §4. |
| Wybór chunkera | po rozszerzeniu pliku (`consts::FIXED_CHUNK_EXTENSIONS`) | Najprostsza reguła spełniająca wymóg. |
| Wersja CID, hash | CIDv1, SHA2-256 | Jak w M1. |
| Odczyt | w głąb, w kolejności linków, z jawnym stosem | Odtwarza także DAG trickle i pliki Kubo z liśćmi dag-pb; głębokość DAG z sieci nie może przepełnić stosu wywołań. |

## 3. Dlaczego nie dag-cbor (propozycja inna niż w wymaganiach)

Wymaganie brzmiało: dag-pb, „albo dag-cbor, jeżeli to możliwe”. Technicznie plik da się zapisać jako drzewo dag-cbor, ale:

1. **Kubo by go nie odczytało jako pliku.** UnixFS v1 to komunikat protobuf `Data` w polu `Data` węzła dag-pb. Dla dag-cbor nie istnieje specyfikacja pliku; `ipfs cat <CID>` zwraca błąd, a zawartość widać tylko przez `ipfs dag get`. Wymóg „to, co wychodzi z crate unixfs, musi być kompatybilne z Kubo” byłby niespełniony.
2. **CID nie dałoby się porównać z Kubo.** Przy dag-pb `ehr-cli add` i `ipfs add` dają ten sam CID, co jest najmocniejszym możliwym testem zgodności (§8).
3. **dag-cbor nic tu nie daje.** Liście raw i tak nie zależą od kodeka, a węzły pośrednie to tylko lista linków.

dag-cbor pozostaje właściwym wyborem dla **metadanych**: manifestów i rejestru EHR (M9) oraz tokenów UCAN (UCAN 1.0 jest kodowany w dag-cbor).

## 4. Reużycie bloków i brak dopełniania

- **Brak dopełniania (paddingu).** Żaden chunker nie dopełnia danych: ostatni kawałek jest po prostu krótszy. Węzły pośrednie nie zawierają bajtów pliku, a liście raw nie mają narzutu.
- **Deduplikacja.** Blockstore przechowuje blok pod jego multihashem. Importer najpierw sprawdza, czy blok już jest (`has`), i dopiero wtedy zapisuje; `add` wypisuje `new_blocks` i `reused_blocks`. `fetch` pobiera z sieci tylko bloki, których nie ma lokalnie (`fetched` / `local`).
- **Dlaczego Rabin.** Przy stałych kawałkach wstawienie jednego bajtu na początku pliku przesuwa wszystkie granice i żaden blok nowej wersji nie pokrywa się ze starą. Rabin wyznacza granice z treści (okno 16 B), więc granice „odnajdują się” za miejscem zmiany.
- **Dlaczego nie domyślne parametry Kubo.** `--chunker=rabin` to `87381-262144-393216`. Zakres min..max jest wąski względem średniej, więc ok. 31% kawałków kończy się na maksimum, czyli cięciem zależnym od pozycji, a po takim cięciu synchronizacja ze starą wersją przepada. Pomiar na pliku tekstowym 1,8 MB po dopisaniu jednej linii na początku:

| Parametry Rabina | Bloki pliku | Bloki reużyte w nowej wersji |
|---|---|---|
| Kubo domyślne `87381-262144-393216` | 6 | **0** |
| Przyjęte `16384-65536-262144` | 9 | **6** |

Przyjęte parametry mają tę samą górną granicę co zwykły blok Kubo (256 KiB), szeroki zakres min..max (wymuszone cięcie w ok. 2% kawałków przy danych losowych) i nadal dają **ten sam CID co Kubo** uruchomione z `--chunker=rabin-16384-65536-262144`. Uwaga: syntetyczny, bardzo powtarzalny tekst z tego pomiaru jest przypadkiem niekorzystnym (mało różnych okien), stąd kawałki bliskie maksimum.

Implementacja Rabina jest portem chunkera Kubo: wielomian `17437180132763653`, okno 16 B, stan zerowany na początku każdego kawałka, maska `2^floor(log2(avg)) - 1`.

## 5. Zakres zgodności UnixFS

| Element | Zapis (Rust tworzy) | Odczyt (Rust odtwarza) |
|---|---|---|
| Plik jednoblokowy raw | tak | tak |
| Plik wieloblokowy, balanced, liście raw, CIDv1 | tak, **CID identyczny z Kubo** | tak |
| Plik pusty | tak (pusty blok raw, jak Kubo) | tak |
| Chunker `size-N` | tak (N = 262144 w CLI) | dowolny |
| Chunker `rabin-min-avg-max` | tak (16384-65536-262144 w CLI) | dowolny |
| Chunker buzhash | nie | tak (odczyt nie zależy od chunkera) |
| Układ trickle | nie | tak |
| Liście dag-pb z danymi w polu `Data` (Kubo bez `--raw-leaves`) | nie | tak |
| Linki CIDv0 wewnątrz DAG | nie | tak |
| Korzeń podany jako CIDv0 (`Qm...`) | nie | nie wprost: trzeba podać `ipfs cid base32 <Qm...>` |
| Katalogi, HAMT, symlinki | nie | nie (`get` zwraca błąd; `fetch` pobiera bloki) |
| Metadane `mode`, `mtime` (UnixFS 1.5) | nie | pomijane |
| Hash inny niż SHA2-256 / SHA2-512 | nie | nie |
| Inline CID (`identity`) | nie | nie |
| Blok większy niż 2 MiB | nie | nie |

Odpowiednik w Kubo dla `ehr-cli add <plik>`:

```text
ipfs add --cid-version=1 --raw-leaves --chunker=size-262144 <audio/wideo>
ipfs add --cid-version=1 --raw-leaves --chunker=rabin-16384-65536-262144 <pozostałe>
```

## 6. CLI i sieć

| Komenda | Działanie |
|---|---|
| `ehr-cli put <plik>` | bez zmian: jeden blok raw, do 2 MiB |
| `ehr-cli add <plik>` | import UnixFS pliku dowolnej wielkości (strumieniowo); `[UNIXFS] ADDED CID=... bytes=... chunker=... new_blocks=... reused_blocks=...` |
| `ehr-cli get <CID> <plik>` | blok raw albo cały plik UnixFS; nie nadpisuje |
| `provide <CID>` (w węźle) | ogłasza **wszystkie bloki** pliku, korzeń na końcu |
| `stop <CID>` | wycofuje wszystkie bloki pliku |
| `fetch <CID>` | pobiera korzeń i wszystkie bloki, do których prowadzą linki; `BITSWAP_FETCH_DONE <CID> fetched=N local=M` |

Zmiany w zachowaniu sieci, które wyszły w testach z Kubo:

- **Korzeń ogłaszany jako ostatni.** Kubo pyta o bloki od razu po znalezieniu korzenia; blok jeszcze nieogłoszony dostawał `DontHave` i pobieranie się zatrzymywało.
- **`fetch` pyta także połączonych peerów**, po dostawcach z DHT. Kubo ogłasza każdy blok dodanego pliku, a rekord korzenia mógł dotrzeć do węzła Rust po zakończeniu zapytania DHT. Tak samo działa Bitswap w Kubo (najpierw połączeni peerzy).
- **Ponawianie otwarcia LevelDB wydłużone z 1 s do 5 s** (`LOCK_RETRY_ATTEMPTS`). Przy pliku wieloblokowym wiele zadań węzła otwiera bazę naraz; po 1 s część żądań Kubo kończyła się błędem blokady i nie dostawała odpowiedzi.

## 7. Kod

| Miejsce | Zawartość |
|---|---|
| `ipfs-unixfs/src/chunker.rs` | `Chunker::{fixed, rabin, for_path, split}`, iterator `Chunks`, tablice Rabina |
| `ipfs-unixfs/src/dagpb.rs` | kanoniczne kodowanie dag-pb (linki przed danymi), linki CIDv0/CIDv1 |
| `ipfs-unixfs/src/importer.rs` | `add_file`: liście → poziomy po 174 → korzeń |
| `ipfs-unixfs/src/exporter.rs` | `cat`, `dag_cids`, `block_links` |
| `ipfs-unixfs/src/models.rs` | `Chunker`, `ImportedFile`, struktury protobuf (`prost`) |
| `ipfs-bitswap/src/client.rs` | `fetch_many`: fala bloków jedną wantlistą, jedno otwarcie bazy do odczytu i do zapisu; `fetch` to skrót dla jednego bloku |
| `ipfs-network/src/bitswap.rs` | `fetch_dag`: przejście po linkach falami po `FETCH_WINDOW` |
| `vendor/libp2p-stream` | kopia z poprawką kolejki przychodzących strumieni (sekcja 9) |
| `ehr-cli` | `add`, `get` przez `ipfs_unixfs::cat`, `provide`/`stop`/`fetch` na całym DAG |

## 8. Testy

| Warstwa | Test | Wynik |
|---|---|---|
| Jednostkowe | `cargo test -p ipfs-unixfs` (14): CID wzorcowe z Kubo dla `size-1024` (2 poziomy, 391 liści), `size-262144`, trzech zestawów Rabina i pliku pustego; granice Rabina; reużycie po wstawce; liście dag-pb i linki CIDv0; kolejność pól dag-pb; błędy | PASS |
| Integracyjne | `integration-unixfs` (2): `add`/`get` dla tekstu i `.mp4`, reużycie bloków między wersjami, brak nadpisywania; pobranie całego pliku między dwoma węzłami, `stop` wszystkich bloków | PASS |
| Interop Kubo 0.43.1 | `scripts/testing/test-kubo-unixfs-interop.sh` (`interop-unixfs`): A ten sam CID w Rust i Kubo (tekst, wideo, mały, pusty) · B Kubo czyta pliki z Rusta (`ipfs cat`) · C Rust odtwarza plik Kubo (rabin) · D DAG dwupoziomowy (200 liści) · E plik CIDv0 z liśćmi dag-pb · F DAG trickle · G katalog odrzucony przez `get` | PASS (3 kolejne uruchomienia) |
| Ręcznie | `M8_VIDEO_BYTES=60000000`: plik 60 MB (229 liści, 2 poziomy), ten sam CID co Kubo, `ipfs cat` z Rusta | PASS |

## 9. Pobieranie falami (wydajność)

`fetch` pobierał bloki po jednym: osobny strumień, osobny `GET` z LevelDB i osobne otwarcie bazy na każdy blok (100 MB = 386 bloków = 46 s, ok. 0,12 s na blok). Zmiany:

| | Zmiana | Efekt |
|---|---|---|
| a | **Wantlista z wieloma wpisami.** Klient wysyła jedną wiadomość z do `FETCH_WINDOW` (64) wpisami Want-Block zamiast jednej wiadomości na blok; serwer przyjmuje do 100 wpisów | jeden strumień zapytań na falę |
| b | **Pobieranie równoległe.** `fetch_dag` idzie falami (BFS): bloki jednej fali są zamawiane naraz, linki z odebranych bloków tworzą następną falę. Korzeń jest zawsze pierwszą, jednoelementową falą | bloki fali płyną jednocześnie |
| c | **Jedno otwarcie bazy na falę.** `BitswapClient::fetch_many` otwiera LevelDB raz do sprawdzenia, które bloki są lokalne, i raz do zapisu wszystkich odebranych (`put_block` nadal liczy skrót każdego bloku) | 2 otwarcia zamiast 2 na blok, mniej konfliktów blokady |

Zachowania, które się nie zmieniły (sprawdzone testami): blok niezgodny z CID jest odrzucany (odpowiedź jest dopasowana po CID wyliczonym z bajtów); `DontHave` i `Timeout` wycofują want (Kubo pamięta want po `DontHave`); po błędzie bloki, które już dotarły, zostają zapisane, więc ponowne `fetch` nie zaczyna od zera; dostawcy są próbowani po kolei, a niepobrane bloki przechodzą do następnego; ten sam blok w DAG jest pobierany raz.

### Poprawka w `libp2p-stream` (potrzebna dla równoległości)

Pierwsza wersja fali z Kubo kończyła się `Bitswap peer timed out`: Kubo wysyła każdy blok osobnym strumieniem, a `libp2p-stream 0.5.0-alpha` ma kolejkę nadchodzących strumieni o **jednym miejscu** (`mpsc::channel(0)`) i **po cichu porzuca** strumienie, gdy kolejka jest pełna (`Channel is full, dropping inbound stream`). Z 5 bloków dochodziły 3. Poprawka jest jednowierszowa (`INBOUND_STREAM_BACKLOG = 256` w `src/shared.rs`) i leży w `vendor/libp2p-stream` (`[patch.crates-io]` w głównym `Cargo.toml`, katalog wyłączony z workspace). Przy aktualizacji tej zależności trzeba poprawkę przenieść albo sprawdzić, czy upstream poszerzył kolejkę. To dotyczy też serwera: seria strumieni od Kubo mogła być gubiona także po tej stronie.

### Pomiary (dwa węzły na jednym komputerze, build release)

Plik losowy `.mp4` (stałe kawałki 256 KiB), węzeł A ogłasza, węzeł B pobiera przez DHT i Bitswap; 18.11.2026.

| Plik | Bloki | `fetch` przed | `fetch` po | Przyspieszenie |
|---|---|---|---|---|
| 10 MB | 40 | 7,6 s | **0,5 s** | 15× |
| 100 MB | 386 | 46 s | **3,6 s** (ok. 28 MB/s) | 13× |

`provide` 100 MB (386 bloków) zajęło 2,0 s. `get` pobranego pliku 100 MB daje plik identyczny bajt po bajcie (`cmp`). Pomiar dotyczy localhost; w sieci rzeczywistej czas zależy od opóźnienia i łącza, ale liczba rund zależy już od liczby fal (głębokość DAG × bloki / 64), a nie od liczby bloków.

## 10. Zabezpieczenia odczytu (`get` / `cat`)

DAG pochodzi z sieci, więc korzeń nie może być zaufany. Spreparowany DAG, w którym każdy poziom linkuje 174 razy ten sam blok, jest kilkaset bajtów w sieci, a po odtworzeniu daje terabajty (albo miliardy odwiedzin bez zapisanych bajtów). `cat` sprawdza:

1. **Limit rozmiaru**: rozmiar deklarowany przez korzeń (`filesize`) nie może przekraczać `MAX_EXPORT_BYTES` (8 GiB); większy plik jest odrzucany **przed zapisem pierwszego bajtu** (`FileTooLarge`).
2. **Rozmiary dzieci**: każdy link ma w rodzicu rozmiar (`blocksizes`) i blok dziecka musi deklarować dokładnie tyle; liczba `blocksizes` musi równać się liczbie linków.
3. **Suma**: własne dane węzła plus suma rozmiarów dzieci musi równać się jego `filesize`.
4. **Brak dzieci o rozmiarze zero**: wyklucza amplifikację bez bajtów (miliardy odwiedzin pustych węzłów).
5. **Budżet bajtów**: zapis nigdy nie przekracza rozmiaru z korzenia (`InvalidDag`); plik jest usuwany przy błędzie (`export_file`).

Skutek: liczba odwiedzonych bloków jest ograniczona rozmiarem pliku, a plik na dysku nie urośnie ponad deklarację korzenia, która sama jest ograniczona limitem. `dag_cids` i `fetch_dag` pomijają powtórzone bloki, więc nie są podatne na ten atak.

Konsekwencja: węzeł dag-pb bez `blocksizes` (np. ręcznie zbudowany albo ze starej implementacji) jest odrzucany. Kubo i importer w tym repozytorium zawsze je zapisują; sprawdzone w interop (pliki balanced, trickle, CIDv0 z liśćmi dag-pb). Testy: `crafted_dags_are_rejected_before_they_grow_the_output` (amplifikacja, kłamiący rodzic, dziecko zerowe, rozmiar ponad limit, uczciwy DAG z powtórzonym blokiem).

## 11. Ograniczenia

- **Blokada LevelDB nadal jest głównym ryzykiem.** Przy pomiarach `provide` zlecone jeden po drugim (`provide A` i od razu `provide B` dużego pliku) dały `STATE_STORE_FAILED` / `DHT_PROVIDE_FAILED`: wątek czytający cały DAG drugiego pliku trzymał bazę, a zapisy stanu pierwszego przekroczyły 5 s oczekiwania. Zlecenia `provide` dużych plików trzeba dawać po kolei i czekać na koniec poprzedniego. Docelowa naprawa: węzeł jest jedynym właścicielem bazy, a komendy CLI przechodzą przez niego.
- Baza LevelDB jest otwierana na czas operacji. `ehr-cli add` dużego pliku oraz `provide`/`fetch` sprawdzające cały DAG trzymają ją przez cały import lub odczyt; operacje działającego węzła, które w tym czasie czekają dłużej niż 5 s, kończą się błędem blokady (`STATE_STORE_FAILED`, `BITSWAP_FAILED`).
- `provide` uruchamia jedno zapytanie DHT na blok; limit ogłaszanych kluczy to 65 536.
- `stop` wycofuje wszystkie bloki pliku, także te współdzielone z innym ogłoszonym plikiem (deduplikacja); taki plik trzeba ogłosić ponownie.
- `get` odrzuca pliki większe niż `MAX_EXPORT_BYTES` (8 GiB); przy większych plikach stałą trzeba podnieść (sekcja 10).
- Każdy zapisany blok wypisuje linie diagnostyczne `[ipfs-cid]` i `[ipfs-store]` na stderr.
- Wybór chunkera po rozszerzeniu nie rozpoznaje audio/wideo w plikach o innej nazwie.

## 10. Budowa DAG: kiedy `root → leaf`, a kiedy `root → node → leaf`

Nie ma osobnej decyzji „węzeł czy liść”. **Liśćmi są zawsze kawałki z chunkera**, a węzły powstają tylko wtedy, gdy liści jest więcej niż jeden: każdy węzeł grupuje do 174 linków. O tym, czy jest `root → leaf`, czy `root → node → leaf`, decyduje wyłącznie **liczba kawałków `n`** porównana z potęgami 174.

### 10.1. Przykład: plik 400 000 B, chunker `size-1024`

Przypadek z testu jednostkowego `balanced_two_level_dag_matches_kubo`; CID zgodny z Kubo.

```text
                              KORZEŃ  bafybeialngtx5…
                              dag-pb, UnixFS File, 3 linki
                              filesize = 400 000
                              blocksizes = [178 176, 178 176, 43 648]
                 ┌──────────────────────┼───────────────────────┐
                 ▼                      ▼                       ▼
            WĘZEŁ A                WĘZEŁ B                 WĘZEŁ C            poziom 1
            dag-pb, 174 linki      dag-pb, 174 linki       dag-pb, 43 linki   (dag-pb)
            178 176 B pliku        178 176 B pliku         43 648 B pliku
        ┌─────┬───┴───┐        ┌─────┬───┴───┐        ┌─────┬───┴───┐
        ▼     ▼       ▼        ▼     ▼       ▼        ▼     ▼       ▼
       L1    L2 …   L174     L175   …     L348      L349   …     L391         liście
       raw   raw    raw      raw          raw       raw          raw          (raw)
      1024  1024   1024     1024         1024      1024         640 B

   bajty pliku:  [L1][L2]…[L174][L175]…[L348][L349]…[L391]   = 400 000 B
```

Kroki budowy (`importer.rs`):

1. **Chunker** tnie plik: 391 kawałków (390 × 1024 B i ostatni 640 B, bez dopełniania).
2. **Liście**: każdy kawałek to blok raw (`bafkrei…`). Przed zapisem importer sprawdza `has()`, więc blok, który już jest w Blockstore, nie jest zapisywany drugi raz.
3. **Poziom 1**: liście grupowane po 174, co daje węzły A (174), B (174) i C (43).
4. **Poziom 2**: trzy węzły mieszczą się w jednym, więc powstaje korzeń i budowa się kończy.

Zawartość jednego węzła dag-pb (np. A):

```text
PbNode                                  ← kolejność bajtów jak w Kubo: Links, potem Data
├─ Links[0]   Hash = CID L1   Name = ""   Tsize = 1024
├─ Links[1]   Hash = CID L2   Name = ""   Tsize = 1024
│  …
├─ Links[173] Hash = CID L174 Name = ""   Tsize = 1024
└─ Data = UnixFS { Type = File, filesize = 178 176, blocksizes = [1024 × 174] }
```

- `blocksizes` mówi, ile bajtów pliku leży pod każdym linkiem. Dzięki temu można przeskoczyć do dowolnego miejsca pliku bez czytania wcześniejszych bloków.
- `Tsize` to łączny rozmiar bloków poddrzewa. Kubo używa go do pokazywania postępu pobierania.
- Węzły nie zawierają bajtów pliku, tylko linki; dane są wyłącznie w liściach.

### 10.2. Ile jest liści

Rozmiar pliku `S` bajtów:

- **stałe kawałki 256 KiB:** `n = ⌈S / 262 144⌉`, a pusty plik daje jeden pusty liść (`n = 1`);
- **Rabin (16–256 KiB):** `n` zależy od treści, ale jest ograniczone: `⌈S / 262 144⌉ ≤ n ≤ ⌊S / 16 384⌋ + 1`.

### 10.3. Poziomy

Algorytm (`importer.rs`, pętla `level.chunks(MAX_FILE_LINKS)`) liczy poziom po poziomie:

```text
n₀ = n                 (liście)
n₁ = ⌈n₀ / 174⌉        (węzły nad liśćmi)
n₂ = ⌈n₁ / 174⌉
…                      aż nₖ = 1  →  to jest korzeń
```

Głębokość drzewa to najmniejsze `d`, dla którego `174^d ≥ n`, czyli `d = ⌈log₁₇₄ n⌉`.

| Liczba liści `n` | Głębokość `d` | Kształt |
|---|---|---|
| 1 | 0 | sam liść raw, bez dag-pb (CID `bafkrei…`) |
| 2 – 174 | 1 | `root → leaf` |
| 175 – 30 276 (=174²) | 2 | `root → node → leaf` |
| 30 277 – 5 268 024 (=174³) | 3 | `root → node → node → leaf` |

Granica leży więc dokładnie przy **174 → 175 liściach**. Przy 174 liściach wszystkie mieszczą się w korzeniu. Przy 175 korzeń nie pomieści już wszystkich linków, więc pod nim powstają węzły pośrednie.

### 10.4. Przykłady

```text
n = 2     → n₁ = ⌈2/174⌉ = 1                         → root → 2 liście            (3 bloki)
n = 174   → n₁ = 1                                   → root → 174 liście          (175 bloków)
n = 175   → n₁ = ⌈175/174⌉ = 2 → n₂ = 1              → root → [174] [1] → liście  (178 bloków)
n = 391   → n₁ = ⌈391/174⌉ = 3 → n₂ = 1              → root → [174] [174] [43]    (395 bloków)
```

Węzły są wypełniane od lewej do pełna, a tylko ostatni na danym poziomie jest niepełny (43 w przykładzie z §10.1). W układzie balanced węzeł danego poziomu linkuje wyłącznie do poziomu niżej, nigdy na raz do liścia i węzła, więc wszystkie liście są na tej samej głębokości. To odróżnia go od trickle.

Liczba wszystkich bloków to `n + n₁ + n₂ + …`. Dla 391 liści wychodzi 391 + 3 + 1 = **395**; dokładnie tyle sprawdza test `balanced_two_level_dag_matches_kubo`.

### 10.5. Maksymalny rozmiar pliku dla głębokości

`S_max(d) = 174^d × rozmiar_kawałka`

| `d` | stałe 256 KiB | Rabin, średnio ~64 KiB |
|---|---|---|
| 1 | 43,5 MiB | ~11 MiB |
| 2 | ~7,4 GiB | ~1,8 GiB |
| 3 | ~1,26 TiB | ~0,3 TiB |

Dla Rabina to wartości orientacyjne, bo długość kawałka zależy od treści.

### 10.6. Skąd 174

To stała Kubo `DefaultLinksPerBlock` z `ipfs/boxo`, plik `ipld/unixfs/importer/helpers/helpers.go`, wyliczona tak, żeby węzeł z linkami miał około 8 KiB:

```text
link ≈ 34 B (multihash SHA2-256) + 8 B (rozmiar) + 5 B (ramka protobuf) = 47 B
8192 B / 47 B ≈ 174,3  →  174
```

Musimy użyć tej samej liczby: przy innej (np. 200) DAG miałby inny kształt i inny CID niż w Kubo, nawet przy identycznych liściach.

### 10.7. Odczyt i reużycie bloków

- **`get` / `cat`** czyta w głąb w kolejności linków: korzeń → A → L1…L174 → B → … → L391. Bajty z kolejnych liści wychodzą od razu w dobrej kolejności.
- **Nowa wersja pliku**: zmieniony kawałek dostaje nowy CID. Nowe CID dostają też węzły na drodze od tego kawałka do korzenia, a wszystkie pozostałe liście i węzły zostają wspólne. Stąd w pomiarze z §4 (plik 1,8 MB, 9 bloków) nowa wersja zapisała tylko 3 nowe bloki, a 6 reużyła.

**Zakres weryfikacji:** z CID Kubo porównano przypadek 391 liści (dwa poziomy) oraz duże pliki z testu interop (§8). Przypadki 2, 174 i 175 liści wynikają z tego samego algorytmu, ale nie mają osobnego testu porównawczego z Kubo.
