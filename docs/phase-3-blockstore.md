# Phase 3 — Blockstore (M2)

## Cel etapu

Implementacja trwałego magazynu bloków IPFS w Rust, wykorzystującego istniejącą bibliotekę `ipfs-cid`, abstrakcję `Datastore` i bazę danych LevelDB. Dokument jest materiałem roboczym do późniejszego opracowania części pracy dyplomowej.

## Schemat działania

### Zapis bloku

```text
Dane pliku (bajty)
    ↓
ipfs-cid: SHA-256 / SHA-512 → Multihash → CIDv1
    ↓
Blockstore: walidacja rozmiaru, CID i integralności
    ↓
Klucz: b"/blocks/" || varint(hash_code) || varint(digest_len) || digest
    ↓
Datastore → LevelDB: atomowy check-and-put (współdzielony uchwyt + Mutex)
    ↓
KEY = prefiks + multihash; VALUE = oryginalne bajty bloku
```

### Odczyt bloku

```text
CIDv1
    ↓
Blockstore: validate_cid() → klucz na podstawie multihash
    ↓
Datastore / LevelDB: get(key)
    ↓
Brak klucza → Ok(None)
Istniejący blok → kontrola rozmiaru + verify_block(bytes)
    ↓
Zgodny digest → Ok(Some(Vec<u8>))
Niezgodny digest → IntegrityMismatch
```

### Decyzje końcowe — stan architektury

1. `ipfs-store` jest podzielone na `Blockstore` (semantyka CID i bloków) oraz `Datastore` (operacje key-value).
2. Backendem `Datastore` jest `LevelDbDatastore` oparty na bibliotece Rust `rusty-leveldb`, a nie pierwotnie rozważany `FilesystemDatastore`.
3. Kluczem LevelDB jest pełny **multihash** poprzedzony lokalnym prefiksem `b"/blocks/"`, a nie pełny CID. Pozwala to deduplikować te same bajty przy różnych kodekach CID.
4. Wartość LevelDB to oryginalne bajty bloku.
5. Przy zapisie i odczycie Blockstore weryfikuje integralność względem CID; `has()` sprawdza tylko obecność klucza.
6. Atomowość operacji `put_if_absent()` wynika ze współdzielonego `Mutex<DB>`; dotyczy wątków korzystających z tego samego uchwytu, nie wielu niezależnych procesów.
7. Maksymalny rozmiar bloku w prototypie wynosi 2 MiB. Nie ma minimalnego rozmiaru: 256 KiB to typowa wielkość fragmentu UnixFS, a nie dolna granica bloku.
8. Testy jednostkowe, integracyjne i interoperacyjności Kubo stanowią odrębny etap walidacji. Przygotowanie kodu testów nie oznacza ich pomyślnego wykonania.

## Notatki źródłowe — zachowane dosłownie

Poniższa sekcja zachowuje pełną treść notatek użytkownika, łącznie z historyczną decyzją dotyczącą `FilesystemDatastore`. Te zapisy służą udokumentowaniu ewolucji architektury. Nie należy traktować ich wszystkich jako aktualnej konfiguracji projektu.

> IPLD (InterPlanetary Linked Data) to ustandaryzowana warstwa modelu danych w ekosystemie IPFS, która łączy różne struktury informacji za pomocą kryptograficznych skrótów (hashy) w jeden globalny graf skierowany (Merkle DAG). Działa jako uniwersalny tłumacz – sprawia, że system może w jednolity sposób adresować (poprzez CID) i przemieszczać przeróżne formaty danych, traktując pliki z UnixFS, obiekty z repozytoriów Git, czy bloki z Ethereum dokładnie tak samo: jako powiązane ze sobą węzły z unikalnym adresem.
>
> Boxo Blockstore (Implementacja referencyjna w Go): W oficjalnym węźle Kubo (wcześniej go-ipfs), cały silnik i interfejsy Blockstore zostały niedawno wydzielone do biblioteki Boxo. To tam znajduje się produkcyjny kod metod takich jak Get, Put, Has, Delete oraz implementacje magazynów pamięciowych i dyskowych.
>
>    Kod źródłowy: [https://github.com/ipfs/boxo/tree/main/blockstore](https://github.com/ipfs/boxo/tree/main/blockstore)
>
>    Datastores (Warstwa poniżej Blockstore): W architekturze IPFS, Blockstore rzadko sam pisze bity na dysk – zazwyczaj deleguje to do niższej warstwy zwanej Datastore (często wykorzystującej systemy takie jak flatfs, badger czy leveldb).
>
>    Interfejsy Datastore: [https://github.com/ipfs/go-datastore](https://github.com/ipfs/go-datastore)
>
>    System plików płaskich (FlatFS) używany domyślnie przez Kubo: [https://github.com/ipfs/go-ds-flatfs](https://github.com/ipfs/go-ds-flatfs)
>
>    Specyfikacja IPLD i IPFS: Oficjalna strona ze specyfikacjami (Specs) opisuje, jak z surowych bloków (Raw Blocks) budowane są struktury logiczne.
>
>    Specyfikacja bloków: [https://ipld.io/specs/codecs/raw/](https://ipld.io/specs/codecs/raw/)
>
>    Ogólna dokumentacja bloków IPFS: [https://docs.ipfs.tech/concepts/how-ipfs-works/#directed-acyclic-graphs-dags](https://docs.ipfs.tech/concepts/how-ipfs-works/#directed-acyclic-graphs-dags)
>    Rust-IPFS (Implementacje w języku Rust): Mimo że oficjalną referencją jest Go, społeczność stworzyła solidne fundamenty w Ryscie, na których warto się wzorować przy projektowaniu własnego Blockstore w ramach pracy inżynierskiej.
>
>    Repozytorium rust-ipfs i ich zarządzanie repozytorium/blokami: [https://github.com/rs-ipfs/rust-ipfs/tree/master/src/repo](https://github.com/rs-ipfs/rust-ipfs/tree/master/src/repo)
>
>
> Interfejs Datastore i jego implementacja FilesystemDatastore.
>
>
>
> Blockstore korzystający z ipfs-cid.
>
>
>
> put_block(cid, data), get(cid), has(cid).
>
>
>
> Jednoznaczne mapowanie CID na klucz dyskowy.
>
>
>
> Atomowy zapis bez nadpisywania poprawnego bloku.
>
>
>
> Kontrola integralności przy zapisie i odczycie.
>
>
>
> Testy trwałości, deduplikacji, korupcji i błędnych danych.
>
> W projekcie zastosowano dwuwarstwową architekturę magazynowania bloków, wzorowaną na podziale Blockstore/Datastore znanym z ekosystemu IPFS. Blockstore odpowiada za operacje na blokach identyfikowanych przez CID oraz kontrolę integralności danych, natomiast Filesystem Datastore zapewnia trwałe przechowywanie bajtów w lokalnym systemie plików. Rozwiązanie celowo pomija zaawansowane optymalizacje produkcyjne, zachowując kluczowe właściwości trwałości, deduplikacji i adresowania zawartością.
>
> LevelDB is a fast, open-source, embedded key-value storage library written in C++ by Google engineers Jeff Dean and Sanjay Ghemawat.
>
> Do zapisywania bajtow plikow po kluczu CID na dysku urzadzenia bedzie wykorzystana biblioteka LevelDB
> LevelDB - C++ opencource Google Library
> Multicodec (raw, dag-pb itd.) nie jest częścią klucza. Dwa CID różniące się kodekiem, ale zawierające ten sam multihash, mogą wskazywać jeden zapisany blok.

## Rozwój decyzji i uzasadnienie

### Od FilesystemDatastore do LevelDB

W pierwszej wersji M2 rozważano własny mini-FlatFS i bezpośredni zapis bloków w osobnych plikach. Po analizie wariantów FlatFS, BadgerDB i LevelDB zdecydowano się na embedded LevelDB jako backend `Datastore`. Umożliwia to skoncentrowanie własnej implementacji na zgodności CID, kontroli integralności, deduplikacji i API Blockstore, zamiast implementowania mechanizmu trwałej bazy danych.

### Dlaczego multihash jest kluczem

Pełny CIDv1 ma pola wersji, kodeka i multihash. Klucz lokalnego magazynu opiera się jedynie na multihash (`code`, `digest length`, `digest`), co oznacza, że CID z różnymi kodekami wskazujące identyczne bajty mogą korzystać z jednego wpisu. Kodek jest nadal zachowany w CID przekazywanym przez aplikację, ale nie jest konieczny do fizycznego odczytu tych bajtów.

### Granice odpowiedzialności

- `ipfs-cid`: generowanie/dekodowanie CID, algorytm haszujący i weryfikacja.
- `Blockstore`: operacje `put`, `put_block`, `get_block`, `has`, sprawdzanie rozmiaru i integralności.
- `Datastore`: ogólny interfejs key-value: `get`, `put`, `has`, `put_if_absent`, `delete`, `keys_with_prefix`.
- `Namespace`: wybór prefiksu klucza („folderu”) dla danego rodzaju danych.
- `LevelDbDatastore`: otwarcie bazy, synchronizacja dostępu przez mutex i odczyt/zapis bajtów.
- `UnixFS`: późniejsze dzielenie plików na bloki, tworzenie i odczyt DAG, rekonstrukcja całego pliku.
- `Bitswap`: późniejszy transfer bloków pomiędzy peerami.
- DHT/libp2p: odkrywanie dostawców i komunikacja sieciowa, a nie lokalny zapis bloku.

### Ograniczenia prototypu

- `put_if_absent()` jest atomowe na poziomie współdzielonego uchwytu bazy z mutexem; nie stanowi transakcji między niezależnymi procesami.
- `flush()` nie powinno być przedstawiane bez dodatkowych dowodów jako gwarancja przetrwania nagłej utraty zasilania.
- Brak jeszcze zarządzania pinningiem, garbage collection, pamięcią podręczną ani MFS. Przestrzenie nazw `/pins/` i `/cache/` są zarezerwowane, ale nie mają jeszcze metod w `Blockstore`.
- Przykładowe testy obejmują trwałość po zamknięciu i ponownym otwarciu oraz porównanie danych z Kubo, lecz ich status należy dokumentować na podstawie rzeczywistego uruchomienia.

## Plan walidacji M2

1. Testy jednostkowe: pusty blok, `Hi!`, granica 2 MiB, błędne CID, niepoprawna długość digestu, integralność, deduplikacja, brak bloku.
2. Testy integracyjne z LevelDB: zamknięcie i ponowne otwarcie bazy, równoległy zapis, błędy otwierania, odczyt atrap binarnych EHR bez ich interpretowania.
3. Interoperacyjność Kubo: Rust → Kubo oraz Kubo → Rust przy porównaniu surowych bajtów i CID, bez kopiowania plików LevelDB.


# Refaktoryzacja M2: stała baza LevelDB wewnątrz ipfs-store

## Założenia

- `ehr-cli` importuje **tylko** `PersistentBlockstore`, bez `LevelDbDatastore` i bez ścieżki bazy.
- `PersistentBlockstore` jest aliasem `Blockstore<LevelDbDatastore>`, aby zachować generyczny Blockstore na potrzeby unit tests i mocków.
- `PersistentBlockstore::open_default()` sam otwiera LevelDB przez `ipfs-store`.
- `DEFAULT_DB_PATH = "data/ehr-blockstore.leveldb"` oraz `LEVELDB_DIR_NAME = "ehr-blockstore.leveldb"` znajdują się wyłącznie w `crates/ipfs-store/src/consts.rs`.
- **Uwaga:** stała ścieżka jest *względna do katalogu bieżącego procesu*; aby używać zawsze tej samej bazy, uruchamiaj CLI z katalogu repozytorium. Jeśli chcesz niezależność od CWD, zamień `DEFAULT_DB_PATH` na absolutną lokalizację odpowiednią dla obu urządzeń lub skorzystaj z aplikacyjnego katalogu danych.
- `open_in(root)` to funkcja pomocnicza dla testów integracyjnych: nazwa bazy jest niezmienna, zmienia się jedynie nadrzędny katalog `tempdir()`. Dzięki temu testy nie kolidują z prawdziwą bazą i mogą działać równolegle.
- Unit tests w `crates/ipfs-store/tests/store-unit-test.rs` pozostają **bez zmian**: korzystają z MemoryDatastore, nie otwierają LevelDB i nie odwołują się do nazwy bazy.
- Testy integracyjne infrastruktury nadal mogą świadomie sięgać do `models::LevelDbDatastore` w scenariuszu sprawdzającym blokadę jednoczesnego otwarcia (lub do `datastore()` w testach korupcji). Kod aplikacji tego nie robi.

## Instalacja

Zrób commit bieżących zmian; rozpakuj ZIP w katalogu głównym repozytorium. Sprawdź diff, zwłaszcza `apps/ehr-cli/src/main.rs`, aby zachować swoje niezacommitowane funkcje.

```
cd ~/ipfs_prototype
unzip -o ~/Downloads/ipfs_store_refactor_patch.zip -d .
cargo fmt --all
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p ipfs-store
cargo test -p test-runner --test integration-store -- --test-threads=1
cargo build -p ehr-cli
```

Nazwa targetu integracyjnego może się różnić: sprawdź `test-runner/Cargo.toml`.

## CLI

```
printf '{"resourceType":"Patient","id":"synthetic"}\n' > /tmp/patient.json
cargo run -p ehr-cli -- cid /tmp/patient.json
cargo run -p ehr-cli -- put /tmp/patient.json
cargo run -p ehr-cli -- has <PEŁNY_CID>
cargo run -p ehr-cli -- get <PEŁNY_CID> /tmp/recovered.json
cmp /tmp/patient.json /tmp/recovered.json && echo PASS
ls -lah data/ehr-blockstore.leveldb
```

Wszystkie `put/has/get` trzeba uruchamiać z tego samego katalogu. Pojedynczy proces otwiera i zamyka bazę po wykonaniu polecenia; nie otwieraj jej równolegle dwoma procesami.

## Uwaga o ograniczeniach

- `get` eksportuje *lokalny* blok, nie pobiera go przez Bitswap.
- Duże pliki ponad 2 MiB nie są w tej fazie obsługiwane jako jeden blok; potrzebny będzie UnixFS.
- `has` potwierdza obecność klucza, nie jest pełną weryfikacją integralności.
- `flush()` nie oznacza automatycznie gwarancji odporności na nagły zanik zasilania.
- Nie dodawaj rzeczywistych danych pacjentów do repo ani do zrzutów ekranu.
- W `.gitignore` dopisz `/data/`, aby nie commitować plików LevelDB.

## Dlaczego nie usuwamy konstruktorów generycznych?

Usunięcie `new_blockstore(D)` wymagałoby przepisywania testów jednostkowych oraz zamknęłoby możliwość testowania logiki Blockstore w pamięci. Fasada aplikacyjna `PersistentBlockstore::open_default()` spełnia wymóg enkapsulacji backendu bez pogorszenia testowalności.


# Rozszerzenie M2: przestrzenie nazw i trwały stan węzła

Stan: kod i testy uruchomione lokalnie 14.11.2026 (`store-unit-test` 24/24, `integration-store` 10/10). Zmiany nie są jeszcze w commicie.

## Przestrzenie nazw („foldery”) w jednej bazie LevelDB

LevelDB nie ma folderów. Rolę folderu pełni **prefiks klucza**, tak jak w datastore Kubo. Wszystkie dane węzła leżą w jednej bazie `data/ehr-blockstore.leveldb`, co daje jedną blokadę `LOCK` i wspólny mechanizm zapisu.

| Przestrzeń (`Namespace`) | Prefiks (`consts.rs`) | Identyfikator | Wartość | Stan |
|---|---|---|---|---|
| `Blocks` | `/blocks/` | multihash | bajty bloku | używane od M2 |
| `Peers` | `/peers/` | PeerId (tekst base58) | tryb DHT + adresy | używane |
| `Provides` | `/provides/` | multihash | bajty CID | używane |
| `RemoteProviders` | `/remote-providers/` | multihash + PeerId | termin ważności (UTC) + adresy | używane |
| `Pins` | `/pins/` | multihash | (do ustalenia) | zarezerwowane |
| `Cache` | `/cache/` | multihash | (do ustalenia) | zarezerwowane |

Zasady implementacji:

- Nazwy prefiksów są stałymi w `crates/ipfs-store/src/consts.rs`.
- `enum Namespace` jest zdefiniowany w `models.rs`, a jego metody w `namespace.rs`.
- `Namespace::prefix()` to **fabryka prefiksów**: jedyny `match` mapujący rodzaj danych na prefiks. `Namespace::key(id)` skleja prefiks z identyfikatorem, `Namespace::owns(key)` sprawdza przynależność klucza.
- Nowy rodzaj danych wymaga trzech kroków: stała prefiksu, wariant `Namespace`, ramię w `match` (kompilator wymusza jego dodanie) oraz wpis w `Namespace::ALL`.
- Układ klucza bloku nie zmienił się (`/blocks/` + `varint(code)` + `varint(len)` + `digest`), więc istniejące bazy działają bez migracji. Pilnuje tego test `block_key_layout_is_unchanged`.
- Pin i cache nie są osobnymi kopiami bloków. Blok zawsze leży w `/blocks/`, a `/pins/` i `/cache/` będą indeksami, na podstawie których zadziała garbage collection.

Aplikacja (`ehr-cli`) i `ipfs-network` nie znają prefiksów. Wywołują metody `Blockstore`, które same wybierają przestrzeń nazw.

## Nowe operacje `Datastore`

| Metoda | Opis |
|---|---|
| `delete(key)` | Usuwa wpis; brak klucza nie jest błędem |
| `keys_with_prefix(prefix)` | Zwraca posortowane klucze zaczynające się od prefiksu |

W `LevelDbDatastore` listowanie używa iteratora LevelDB: `seek(prefix)`, a potem odczyt kolejnych kluczy do pierwszego klucza spoza prefiksu. Klucze w LevelDB są posortowane, więc wpisy jednej przestrzeni nazw leżą obok siebie.

## Nowe metody publiczne `Blockstore`

| Metoda | Przestrzeń | Opis |
|---|---|---|
| `save_peer(&KnownPeer)` | `Peers` | Zapis lub nadpisanie znanego peera, z walidacją pól |
| `load_peers()` | `Peers` | Lista zapamiętanych peerów posortowana po PeerId |
| `remove_peer(peer_id)` | `Peers` | Usunięcie wpisu |
| `add_provide(&Cid)` | `Provides` | Zapamiętanie, że węzeł ogłasza CID w DHT |
| `remove_provide(&Cid)` | `Provides` | Wycofanie wpisu (po `stop <CID>`) |
| `list_provides()` | `Provides` | CID do ponownego ogłoszenia po restarcie |
| `save_remote_provider(&RemoteProvider)` | `RemoteProviders` | Zapis rekordu „peer ma blok”; ponowny zapis odświeża termin |
| `load_remote_providers(now_utc)` | `RemoteProviders` | Tylko rekordy, których termin jeszcze nie minął |
| `remove_expired_remote_providers(now_utc)` | `RemoteProviders` | Usuwa przeterminowane rekordy, zwraca ich liczbę |

`KnownPeer { peer_id: String, addresses: Vec<String>, dht_server: bool }` używa typów tekstowych, bo `ipfs-store` nie zależy od libp2p. Wartość wpisu ma format liniowy: pierwsza linia `dht=server` albo `dht=client`, kolejne linie to adresy Multiaddr. Uszkodzony wpis daje błąd `CorruptRecord`, a nieprawidłowe dane wejściowe `InvalidPeer`.

`RemoteProvider { multihash: Vec<u8>, peer_id: String, addresses: Vec<String>, expires_at_utc: u64 }` opisuje rekord otrzymany z DHT. Klucz to prefiks + multihash + PeerId; multihash sam określa swoją długość, więc klucz da się jednoznacznie rozdzielić. Pierwsza linia wartości to `expires_utc=<sekundy>`, kolejne to adresy.

### Czas zawsze w UTC

Wszystkie znaczniki czasu zapisywane przez aplikację to **sekundy od epoki Unix w UTC**. Źródłem jest `utils::utc_now_secs()`. Wynik nie zależy od strefy czasowej urządzenia, więc rekord zapisany na jednym komputerze znaczy to samo na każdym innym. Metody magazynu przyjmują `now_utc` jako argument, dzięki czemu testy nie zależą od zegara systemowego.

## Kto korzysta z magazynu stanu

`ipfs-network` zależy teraz od `ipfs-store` i otwiera bazę przez `PersistentBlockstore::open_in(katalog)` **tylko na czas pojedynczej operacji** (zapis peera, zapis CID, odczyt przy starcie). Dzięki temu działający węzeł nie blokuje poleceń `ehr-cli put|has|get` uruchamianych w drugim terminalu. Jeśli dwie operacje trafią na siebie, węzeł wypisuje `STATE_STORE_FAILED` i ponawia zapis peera przy następnym Identify. Szczegóły zachowania sieci opisuje `phase-5-dht.md`, sekcja „Trwały stan węzła”.

Stała `DEFAULT_DATA_DIR = "data"` wskazuje katalog danych; `open_in(DEFAULT_DATA_DIR)` otwiera tę samą bazę co `open_default()`.

## Testy

| Test | Warstwa | Co sprawdza |
|---|---|---|
| `namespace_prefixes_match_consts_and_are_disjoint` | jednostkowa | Prefiksy zgodne ze stałymi, przestrzenie rozłączne |
| `block_key_layout_is_unchanged` | jednostkowa | Zgodność wsteczna klucza bloku |
| `same_id_in_other_namespace_does_not_collide_with_block` | jednostkowa | Ten sam multihash w `Pins` nie nadpisuje bloku |
| `peers_are_saved_listed_overwritten_and_removed` | jednostkowa | Cykl życia wpisu peera |
| `invalid_or_corrupt_peer_is_rejected` | jednostkowa | Walidacja i wykrywanie uszkodzonego wpisu |
| `provides_are_tracked_separately_from_blocks` | jednostkowa | Lista ogłaszanych CID niezależna od bloków |
| `remote_providers_are_saved_refreshed_and_expire_by_utc_time` | jednostkowa | Zapis, odświeżenie terminu, wygasanie i porządkowanie |
| `invalid_or_corrupt_remote_provider_is_rejected` | jednostkowa | Walidacja multihash i PeerId, uszkodzony wpis |
| `utc_clock_returns_unix_seconds` | jednostkowa | Zegar zwraca sekundy Unix |
| `peers_and_provides_persist_across_reopen_without_touching_blocks` | integracyjna (LevelDB) | Trwałość po zamknięciu i ponownym otwarciu bazy |
| `remote_providers_persist_across_reopen` | integracyjna (LevelDB) | Trwałość rekordów providerów i ich wygasanie |

## Ograniczenia

- Brak limitu liczby zapamiętanych peerów i brak usuwania nieaktualnych wpisów.
- Zapis peera i zapis bloku to osobne operacje; nie ma jeszcze zapisu wsadowego (batch).
- Pinning, cache i garbage collection pozostają do zaimplementowania.


## Literatura

- [Boxo Blockstore](https://github.com/ipfs/boxo/tree/main/blockstore)
- [go-datastore](https://github.com/ipfs/go-datastore)
- [go-ds-flatfs](https://github.com/ipfs/go-ds-flatfs)
- [IPLD Raw Codec](https://ipld.io/specs/codecs/raw/)
- [IPFS Merkle DAG](https://docs.ipfs.tech/concepts/how-ipfs-works/#directed-acyclic-graphs-dags)
- [rust-ipfs repo](https://github.com/rs-ipfs/rust-ipfs/tree/master/src/repo)
- [LevelDB implementation notes](https://github.com/google/leveldb/blob/main/doc/impl.md)
