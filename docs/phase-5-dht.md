# Phase 5 — Kademlia DHT: routing i provider discovery (M5, Issues #6 i #7)

> **Stan dokumentacji:** 12.11.2026. Opis odpowiada kodowi na gałęzi `master` (commit `d005c75` „#dht added DHT and Kademilia to nodes”). Testy jednostkowe, integracyjne (regresja M4) oraz interoperacyjności Rust ↔ Kubo zostały uruchomione i **zakończyły się powodzeniem**.

## 1. Cel i zakres

**M5 / Issue #6 — Integrate Kademlia DHT routing**

- Rozszerzyć istniejący `ipfs-network` o `libp2p::kad`, bez tworzenia drugiego `Swarm` ani zastępowania warstwy transportowej.
- Utrzymywać tablice routingu Kademlia (`k-buckets`), przetwarzać zdarzenia DHT i uruchamiać bootstrap routingu po poznaniu kompatybilnych węzłów.
- Umożliwić poznawanie kolejnych uczestników poprzez zapytania DHT, bez konieczności ręcznego zapisywania adresu każdego peera.

**M5 / Issue #7 — Add DHT provider discovery and Kubo test**

- Udostępnić `start_providing`, `get_providers` i `stop_providing`.
- Odnajdywać `PeerId` węzłów ogłaszających posiadanie bloku o wskazanym CID.
- Przetestować routing i provider discovery między węzłami Rust oraz z Kubo.

**Granica odpowiedzialności M5:** DHT ustala, *kto deklaruje posiadanie bloku* i pomaga znaleźć peera. Nie przesyła treści bloku, nie przyznaje uprawnień medycznych i nie zastępuje Bitswap (M6/M7).

## 2. Decyzje architektoniczne

| Zagadnienie | Ostateczna decyzja dla M5 |
|---|---|
| Biblioteka | Zachowujemy wersję `libp2p = "0.57.1"` z projektu; dodajemy feature `kad`. |
| Sieć DHT | Wyłącznie LAN DHT: `/ipfs/lan/kad/1.0.0`. |
| WAN | Nie jest implementowany ani wybierany flagą. |
| Środowisko | Testy pod jednym routerem; przyszłościowo także przez routowany VPN. |
| Architektura | Jeden dotychczasowy `Swarm`, `NetworkBehaviour` i event loop. |
| Transport | Istniejące TCP + Noise + Yamux. |
| Tożsamość | Ten sam trwały klucz Ed25519 i `PeerId` z M4. |
| Ścieżka tożsamości | Stała `./tmp/node-key.pb`; **bez flagi** do podawania ścieżki. |
| Pacjent | DHT `Client` domyślnie. |
| Placówka | DHT `Server` po uruchomieniu z `--dht-server`. |
| AutoNAT | Nie implementujemy w M5. |
| Bootstrap CLI | `node start` wybiera istniejący, zapisany na stałe seed; można wskazać własny lub jawnie pusty bootstrap. |
| Danych medycznych | Nie udostępniamy rzeczywistych dokumentów w testach; stosujemy sztuczne bloki. |

DHT **Client/Server** nie jest rolą biznesową **Pacjent/Placówka**. DHT Server odpowiada na zapytania routingu, ale nie uzyskuje w ten sposób uprawnień do dokumentacji EHR. Client może inicjować połączenia TCP, odbierać połączenia transportowe i wysyłać zapytania DHT, lecz nie obsługuje zapytań Kademlia od innych peerów.

### 2.1. Dlaczego LAN zamiast WAN?

Kubo rozróżnia protokoły:

- LAN: `/ipfs/lan/kad/1.0.0` — DHT dla prywatnych adresów i sieci lokalnych.
- WAN: `/ipfs/kad/1.0.0` — publiczny DHT Amino, z zasadami kwalifikowania publicznie osiągalnych adresów.

W naszym prototypie węzły mają prywatne adresy IPv4 i działają za jednym routerem. LAN DHT pozwala testować ich routing bez wystawiania urządzeń do publicznego Internetu. Nie potrzebujemy implementacji WAN do spełnienia M5 w przyjętym zakresie.

Docelowe połączenie węzłów z różnych miast można zrealizować przez **VPN warstwy 3**, który udostępnia wzajemnie routowane adresy VPN. Nadal można używać tego samego protokołu LAN DHT, jeśli dostępne są odpowiednie adresy i bootstrap. VPN nie jest tym samym co VLAN. W tej wersji **nie implementujemy** konfiguracji VPN ani automatycznego wykrywania NAT.

> LAN DHT **nie oznacza** automatycznego wykrywania wszystkich urządzeń Wi-Fi. Kademlia nie skanuje podsieci i w M5 nie dodajemy mDNS. Pierwszy peer musi zostać wskazany przez bootstrap albo musi sam nawiązać połączenie.

## 3. Architektura `ipfs-network`: M4 → M5

### 3.1. Stan po M4

```text
NetworkConfig (listen, identity_path, bootstrap, retry)
       |
Identity (Ed25519 -> stabilny PeerId)
       |
Swarm (TCP + Noise + Yamux)
       |
NetworkBehaviour (Identify + Ping)
       |
Event loop (Connect / Disconnect / Identify / Ping / DialFailed)
       |
Retry state (bootstrap peers + exponential backoff)
```

### 3.2. Stan po rozszerzeniu o M5

```text
NetworkConfig (+ dht_mode)
       |
Identity (bez zmian)
       |
Swarm (TCP + Noise + Yamux; bez zmian)
       |
NetworkBehaviour
    |-- Identify
    |-- Ping
    `-- Kademlia DHT (MemoryStore)
           |-- k-buckets / bootstrap
           |-- start_providing / get_providers / stop_providing
           `-- zdarzenia zapytań i aktualizacji routingu
       |
Istniejący NetworkNode / event loop
       |-- reconnect M4
       |-- obsługa NetworkEvent::Kad
       `-- komendy DHT z ehr-cli (mpsc)
```

**Reużycie:** istniejący `identity.rs`, `error.rs`, `utils.rs`, `ipfs-cid`, Blockstore, konfiguracja transportu, struktury retry i mechanizmy Ping/Identify. DHT nie wymaga drugiego procesu `ipfs-dht-node` ani własnego parsera CID.

### 3.3. Zakres zmian w plikach

| Plik | Zmiana / rola |
|---|---|
| `crates/ipfs-network/Cargo.toml` | Feature `kad` i zależności potrzebne do klucza providera. |
| `src/consts.rs` | Stałe protokołu LAN DHT i timeoutu zapytania; zachowany dotychczasowy domyślny bootstrap i tożsamość. |
| `src/dht.rs` | `DhtMode`, `KademliaBehaviour`, `build_kademlia`, `provider_key`. Bez testów w tym pliku (testy w `tests/dht-unit-test.rs`). |
| `src/behaviour.rs` | Pole `kad` oraz `NetworkEvent::Kad` obok Identify i Ping. |
| `src/models.rs` | `dht_mode`, stan zapytań providerów, `NetworkCommand`. |
| `src/config.rs` | Domyślny `DhtMode::Client`; konfiguracja biblioteki pozostaje bez bootstrapu dopóki CLI go nie wskaże. |
| `src/node.rs` | Integracja Kademlia z istniejącym Swarm, routing, Identify, bootstrap DHT, komendy, wyniki zapytań. |
| `src/lib.rs` | Eksport modułu DHT i typów potrzebnych CLI. |
| `crates/ipfs-network/Cargo.toml` (testy) | Rejestracja targetu `[[test]] dht-unit-test`. |
| `crates/ipfs-network/tests/dht-unit-test.rs` | Testy trybów, protokołu, konstrukcji Kademlia i klucza providera. |
| `apps/ehr-cli/src/main.rs` | Ustalona składnia startu, `--dht-server`, interaktywne komendy DHT (wątek stdin → `mpsc` → `NetworkNode`). |
| `test-runner/Cargo.toml` | Nowy target `[[test]] interop-dht`. |
| `test-runner/tests/interoperability/dht_interop.rs` | Test Rust uruchamiający skrypt interoperacyjności z Kubo. |
| `scripts/testing/test-kubo-dht-interop.sh` | Scenariusze A/B/C: Rust → Kubo, Kubo → Rust, Rust → Rust przez sieć DHT z Kubo. |
| `test-runner/tests/integration/network_integration.rs` | Korekta wcześniejszego przypadku, który traktował `node start` bez argumentów jako błąd. |

Pliki `identity.rs`, `error.rs`, `utils.rs` oraz kod Blockstore i Bitswap nie są częścią zmian M5.

## 4. Routing Kademlia: w jaki sposób poznajemy nowe węzły?

### 4.1. Bootstrap transportowy a bootstrap DHT

1. Uruchamiany węzeł wczytuje swój trwały klucz i tworzy `Swarm`.
2. Jeśli konfiguracja zawiera bootstrap peer, `node.rs` rejestruje jego `PeerId` i `Multiaddr` w Kademlia przez `kad.add_address`.
3. Istniejący mechanizm M4 wykonuje dial i ewentualny reconnect z backoff.
4. Identify przekazuje listę protokołów i adresów nasłuchiwania połączonego peera.
5. Jeżeli peer ogłasza `/ipfs/lan/kad/1.0.0`, jego adresy są dodawane do Kademlia.
6. `kad.bootstrap()` uruchamia zapytania pozwalające rozbudowywać tablice routingu o kolejnych kandydatów.
7. `RoutingUpdated` sygnalizuje aktualizacje `k-buckets`.

**Wniosek:** bootstrap peer to punkt wejścia, **nie** centralny serwer dokumentacji EHR ani serwer autoryzacji. Pierwszy Server z pustym bootstrapem nasłuchuje i nie inicjuje sam odkrywania obcych węzłów. Poznaje innych dopiero dzięki połączeniom przychodzącym i dalszej wymianie komunikatów routingu.

### 4.2. Metryka XOR i `k-buckets`

DHT lokuje węzły i klucze w logicznej przestrzeni 256-bitowej. Dla identyfikatorów routingu `A` i `B`:

```text
d(A, B) = A XOR B
```

Wynik XOR traktujemy jako nieujemną liczbę całkowitą. Im mniejszy wynik, tym bliżej siebie znajdują się identyfikatory **logicznie**. W praktyce IPFS sprowadza klucze i PeerId do 256-bitowych identyfikatorów routingu za pomocą SHA-256 ich reprezentacji bajtowej. DHT odpyta znane peery i będzie poszukiwać kolejnych, bliższych szukanemu kluczowi.

**XOR nie mierzy geografii ani opóźnienia.** Węzeł po drugiej stronie świata może być bliższy w przestrzeni DHT niż komputer w tej samej sieci Wi-Fi. Kademlia korzysta z równoległych zapytań i tablic routingu; w M5 nie dodajemy osobnej polityki wyboru peerów według RTT.

## 5. Provider discovery: `CID` → `PeerId`

### 5.1. Klucz rekordu dostawcy

Rekord providera w IPFS DHT jest indeksowany **multihashem**, a nie pełnym CID:

```text
CIDv1  = varint(version) || varint(codec) || multihash
multihash = varint(hash_function) || varint(digest_length) || digest
DHT provider key = multihash
```

Moduł DHT wykorzystuje istniejący `ipfs_cid::models::Cid`. `provider_key()` pobiera bajty CID i pomija wersję oraz kodek, pozostawiając multihash. Nie powstaje drugi parser CID.

CID typu `raw` i `dag-pb` z takim samym multihashem mają ten sam klucz providera. **Uwaga:** jest to ten sam *fragment multihasha* używany przez Blockstore, ale fizyczny klucz bazy Blockstore może zawierać dodatkowy prefiks (np. `/blocks/`); nie należy utożsamiać obu pełnych kluczy bazy danych.

### 5.2. Przebieg ogłaszania i wyszukiwania

```text
Placówka A           Kademlia Server B          Pacjent C (Client)
     |                       |                         |
     | start_providing(CID)  |                         |
     |-- ADD_PROVIDER ------>|                         |
     |                       |                         |
     |                       |<-- GET_PROVIDERS(CID) ---|
     |                       |--- PeerId placówki A --->|
     |                       |                         |
```

Schemat pokazuje uproszczony przypadek, w którym B jest jednym z węzłów posiadających rekord providera. Faktyczna Kademlia kieruje zapytania według odległości XOR; nie gwarantuje, że zawsze odpytany zostanie właśnie B.

- `start_providing(CID)` zapisuje lokalną informację o dostawcy i inicjuje ogłaszanie jej do DHT.
- `get_providers(CID)` inicjuje zapytanie; wyniki mogą przychodzić przyrostowo.
- `stop_providing(CID)` zatrzymuje lokalne ogłaszanie; wcześniejsze rekordy mogą pozostawać u innych peerów do wygaśnięcia.
- `DHT_PROVIDER <PeerId>` to informacja o znalezionym **zdalnym** dostawcy. Wyszukiwanie filtruje własny `PeerId` węzła.
- `DHT_FIND_FINISHED count=N` sygnalizuje zakończenie śledzonego zapytania i liczbę zebranych unikalnych zdalnych dostawców.

W `ehr-cli` polecenie `provide <CID>` najpierw sprawdza przez istniejący Blockstore, czy lokalnie przechowujemy blok. Nie wolno utożsamiać poprawnego `find` z możliwością pobrania danych: to dopiero zadanie Bitswap.

## 6. Tryby DHT Client / Server

| Cecha | Client (pacjent, domyślnie) | Server (placówka, `--dht-server`) |
|---|---|---|
| Inicjowanie połączeń libp2p | Tak | Tak |
| Przyjmowanie połączeń libp2p | Tak, jeśli adres jest osiągalny | Tak, jeśli adres jest osiągalny |
| Ping / Identify | Tak | Tak |
| Wychodzące zapytania Kademlia | Tak | Tak |
| Odpowiadanie na zapytania routingu Kademlia | Nie | Tak |
| `get_providers` | Tak | Tak |
| `start_providing` | Możliwe | Możliwe |
| Uprawnienia medyczne | Wynikają z warstwy autoryzacji, nie z trybu DHT | Tak samo |

Przełączanie odbywa się **przy uruchomieniu**. Aktualna wersja M5 nie ma AutoNAT ani automatycznej polityki Client → Server. Tryb `Server` nie zapewnia samodzielnie osiągalności: wymagany jest dostępny adres, działające nasłuchiwanie i poprawne reguły sieciowe.

## 7. Interfejs `ehr-cli` i bootstrap

### 7.1. Stała tożsamość i konfiguracja sieci

```text
DEFAULT_IDENTITY_PATH = ./tmp/node-key.pb
DEFAULT_LISTEN_MULTIADDR = /ip4/127.0.0.1/tcp/0
DEFAULT_BOOTSTRAP_MULTIADDR = /ip4/192.168.1.125/tcp/4101/p2p/<ZAPISANY_PEER_ID>
```

Wartości pochodzą z `crates/ipfs-network/src/consts.rs`. Domyślny bootstrap zawiera **konkretny PeerId**: węzeł pod wskazanym IP musi używać odpowiadającego mu klucza. Jeżeli testowy serwer używa innego PeerId, na pozostałych urządzeniach należy wskazać jego rzeczywisty adres `--bootstrap`.

Nie dodajemy flagi `--identity-path`. Plik klucza jest odczytywany lub tworzony pod domyślną ścieżką, względną wobec bieżącego katalogu roboczego. Przy testach kilku procesów na **jednym komputerze** należy użyć osobnych katalogów roboczych; na trzech komputerach pliki tożsamości są niezależne.

### 7.2. Polecenia startowe

| Polecenie | Zachowanie |
|---|---|
| `node start` | Domyślny nasłuch, domyślny bootstrap, DHT Client. |
| `node start --dht-server` | Domyślny nasłuch, domyślny bootstrap, DHT Server. |
| `node start "" --dht-server` | Domyślny nasłuch, brak bootstrapu, DHT Server. |
| `node start <listen> "" --dht-server` | Wskazany nasłuch, brak bootstrapu; pierwszy Server w sieci. |
| `node start <listen>` | **Zgodność z M4:** tylko nasłuch, bez bootstrapu. |
| `node start <bootstrap/p2p/PeerId>` | Domyślny nasłuch, wskazany bootstrap. |
| `node start <listen> --bootstrap <bootstrap/p2p/PeerId>` | Wskazany nasłuch i wskazany bootstrap. |
| `node start <listen> --default-bootstrap` | Wskazany nasłuch i zahardkodowany bootstrap. |

Flaga `--dht-server` może być dodana do odpowiedniego polecenia. Nie ma flag WAN, LAN, AutoNAT ani osobnej flagi tożsamości. Na trzech urządzeniach testowych jawnie ustawiamy adres LAN do nasłuchu.

### 7.3. Polecenia w działającym procesie `node start`

```text
peers             wyświetl wpisy znane lokalnej tablicy routingu Kademlia
bootstrap         ponów zapytanie bootstrap DHT
provide <CID>     ogłoś CID istniejący w lokalnym Blockstore
find <CID>        znajdź deklarujących dostawców bloku
stop <CID>        zatrzymaj lokalne ogłaszanie
quit              zakończ aplikację
```

Komendy są przekazywane z terminala przez kanał `tokio::sync::mpsc` do istniejącej pętli `NetworkNode`. Nie powstaje drugi `Swarm`. `peers` nie musi wyświetlać każdego połączonego klienta: reprezentuje tablice routingu DHT, a nie pełną listę wszystkich połączeń transportowych.

## 8. Przygotowanie testów

1. Zainstaluj Rust/Cargo na komputerach A, B i C, skopiuj tę samą wersję kodu i wykonaj `cargo build -p ehr-cli`.
2. Połącz urządzenia z **tą samą siecią LAN**. Wyłącz izolację klientów Wi-Fi (AP/client isolation), jeżeli router ją stosuje.
3. Ustal adresy IPv4. Poniżej używamy **przykładów**: A=`192.168.1.125`, B=`192.168.1.126`, C=`192.168.1.127`. Podstaw rzeczywiste adresy swoich urządzeń.
4. Dopuść połączenia TCP na porcie `4101` pomiędzy urządzeniami (firewall). Ten sam port może być używany na trzech różnych komputerach.
5. Każdy komputer uruchamia proces z katalogu głównego **własnej** kopii repozytorium. W ten sposób `./tmp/node-key.pb` pozostaje niezależne.
6. Nie używaj rzeczywistych danych pacjentów. Utwórz fikcyjny plik testowy.

### 8.1. Test 1 — Placówka A: pierwszy DHT Server bez bootstrapu

Na komputerze **A** utwórz blok w lokalnym Blockstore (zanim uruchomisz węzeł):

```bash
printf 'EHR M5 TEST: fake medical record, no personal data\n' > "$PWD/m5-demo.txt"
cargo run -p ehr-cli -- put "$PWD/m5-demo.txt"
```

Zapisz `CID` z wyniku `[BLOCKSTORE] PUT CID=...`.

Uruchom serwer **A**:

```bash
cargo run -p ehr-cli -- node start /ip4/192.168.1.125/tcp/4101 "" --dht-server
```

Zapisz `LOCAL_PEER_ID` jako `PEER_A`. Oczekiwane elementy logu:

```text
[NETWORK] LISTEN /ip4/192.168.1.125/tcp/4101
[NETWORK] LISTEN ONLY (no bootstrap)
[DHT] LAN Server
LOCAL_PEER_ID <PEER_A>
LISTEN /ip4/192.168.1.125/tcp/4101/p2p/<PEER_A>
```

**Zrzut S1:** A działa jako Server, ma stabilny PeerId i nie korzysta z bootstrapu.

### 8.2. Test 2 — Placówka B: drugi DHT Server dołącza do A

Na komputerze **B**:

```bash
cargo run -p ehr-cli -- node start /ip4/192.168.1.126/tcp/4101 \
  --bootstrap /ip4/192.168.1.125/tcp/4101/p2p/PEER_A \
  --dht-server
```

Wstaw rzeczywisty `PEER_A` bez nawiasów. Zapisz `LOCAL_PEER_ID` B jako `PEER_B`.

Oczekiwane fragmenty logów, gdy sieć działa:

```text
CONNECTED <PEER_A>
IDENTIFY <PEER_A>: ...
DHT_ROUTING_UPDATED <PEER_A> new=true
DHT_BOOTSTRAP_STARTED
```

Na **A** może pojawić się `CONNECTED <PEER_B>` i `DHT_ROUTING_UPDATED <PEER_B>`. Na obu serwerach wpisz `peers` i odszukaj `DHT_PEER` drugiego serwera.

**Zrzut S2:** B połączył się z A, a Kademlia utworzyła/uzupełniła tablicę routingu.

### 8.3. Test 3 — Pacjent C: Client zna tylko serwer B

Na komputerze **C**:

```bash
cargo run -p ehr-cli -- node start /ip4/192.168.1.127/tcp/4101 \
  --bootstrap /ip4/192.168.1.126/tcp/4101/p2p/PEER_B
```

**Bez `--dht-server`**: pacjent działa jako Client. W konfiguracji bootstrap C wpisujemy **tylko B**, a nie A. W logach sprawdź:

```text
[DHT] LAN Client
CONNECTED <PEER_B>
DHT_BOOTSTRAP_STARTED
```

Wpisz `peers`. Obecność `DHT_PEER <PEER_B>` pokazuje, że C zna serwer routingu. Zależnie od postępu wyszukiwania lista może zawierać także A.

**Zrzut S3:** Client i jego połączenie z B, bez ręcznej konfiguracji połączenia do A.

### 8.4. Test 4 — Provider discovery: C odnajduje blok A przez DHT

Dopiero po zestawieniu połączenia A ↔ B wróć na **A** i wpisz CID zapisany z `put`:

```text
provide <CID_Z_PUT_NA_A>
```

Prawidłowy przebieg:

```text
DHT_PROVIDE_REQUESTED <CID>
DHT_PROVIDING_STARTED ...
```

Komunikat `DHT_PROVIDING_STARTED` oznacza wynik opublikowania rekordu zgłoszony przez Kademlia, a nie przesłanie zawartości bloku.

Następnie na **C** wpisz dokładnie ten sam CID:

```text
find <CID_Z_PUT_NA_A>
```

Oczekiwany wynik:

```text
DHT_FIND_REQUESTED <CID>
DHT_PROVIDER <PEER_A>
DHT_FIND_FINISHED count=1
```

Liczba może być większa, jeśli istnieją inni zdalni dostawcy tego samego multihasha. W tym kontrolowanym teście oczekujemy przynajmniej `DHT_PROVIDER <PEER_A>`. Jeśli rekord nie został jeszcze rozpropagowany, sprawdź logi `DHT_PROVIDING_*`, tablice routingu i ponów `find`.

**Zrzut S4 (najważniejszy):** na jednym ujęciu lub dwóch zestawionych zdjęciach pokaż `provide` na A i `DHT_PROVIDER <PEER_A>` na C. Demonstruje to odnalezienie dostawcy, którego C **nie musiał** wpisywać jako bootstrap.

### 8.5. Test 5 — Poprawność Client/Server oraz brak przesyłania bloku

Na **C** wykonaj:

```bash
# w osobnym terminalu, w tym samym katalogu roboczym
cargo run -p ehr-cli -- has <CID_Z_PUT_NA_A>
```

Oczekiwane `HAS false` pokazuje, że `find` nie pobrało bloku z A. To **prawidłowe zachowanie M5**, nie błąd. Nie uruchamiaj na C `get <CID>` w oczekiwaniu na zdalny transfer: obecne `get` odczytuje lokalny Blockstore.

**Zrzut S5:** `DHT_PROVIDER <PEER_A>` w terminalu `node start` na C oraz `[BLOCKSTORE] HAS false` w drugim terminalu na C.

### 8.6. Test 6 — Reconnect po restarcie A (regresja M4)

Na A zakończ proces poleceniem `quit`, pozostawiając B uruchomiony. Obserwuj `DISCONNECTED <PEER_A>` i zaplanowany retry na B. Uruchom A ponownie **z tego samego katalogu** i z tą samą domyślną tożsamością:

```bash
cargo run -p ehr-cli -- node start /ip4/192.168.1.125/tcp/4101 "" --dht-server
```

`LOCAL_PEER_ID` A powinien być taki sam jak przed restartem. Na B oczekuj ponownego `CONNECTED <PEER_A>`. Po restarcie A **ponów `provide <CID>`**: używany `MemoryStore` nie zachowuje lokalnie ogłaszanych CID po restarcie.

**Zrzut S6:** B ponownie łączy się z A, PeerId A pozostaje niezmieniony. To weryfikuje kompatybilność M5 z reconnect z M4.

## 9. Testy automatyczne

### 9.1. Testy logiki i CLI

```bash
cargo fmt --all
cargo check --workspace --all-targets
cargo test -p ipfs-network --test dht-unit-test
```

`dht-unit-test.rs` sprawdza:

| Test | Co weryfikuje |
|---|---|
| `dht_uses_lan_protocol` | Identyfikator `/ipfs/lan/kad/1.0.0` i dodatni timeout zapytań |
| `client_is_default_and_server_can_be_selected` | `NetworkConfig::default()` → Client; mapowanie na `kad::Mode` |
| `kademlia_can_be_built_in_both_modes` | Budowa `kad::Behaviour` w trybie Client i Server |
| `provider_key_is_multihash_without_cid_prefix` | Klucz = 34 B: `0x12 0x20` + digest SHA-256 |
| `different_cid_codecs_share_provider_key_if_multihash_matches` | `raw` i `dag-pb` z tym samym multihashem → ten sam klucz |
| `invalid_multihash_length_is_rejected` | Odrzucenie niespójnego `size` multihasha |

Parsowanie opcji `node start` jest weryfikowane procesowo w `integration-network` (§9.4), bez osobnego modułu testów w `ehr-cli`.

### 9.2. Kilka procesów Rust

Scenariusz wielu węzłów Rust na jednym komputerze realizuje skrypt z §9.3 (scenariusz **C**: dwa niezależne procesy Rust z osobnymi katalogami roboczymi, kluczami i Blockstore oraz węzeł Kubo jako DHT Server). Scenariusz trzech fizycznych komputerów opisano w §8.

### 9.3. Interoperacyjność Rust ↔ Kubo

```bash
bash scripts/testing/test-kubo-dht-interop.sh

# Alternatywnie test Rust uruchamiający ten sam skrypt:
cargo test -p test-runner --test interop-dht -- --nocapture --test-threads=1
```

Zmienne środowiskowe skryptu: `M5_TEST_IP` (domyślnie `127.0.0.1`), `M5_KUBO_PORT` (`4201`), `M5_RUST_PORT` (`4301`). Węzły Rust startują z `--dht-server` i bootstrapem do Kubo; skrypt czeka na `CONNECTED <KUBO_ID>` i `DHT_BOOTSTRAP_STARTED`, a komendy podaje przez FIFO na stdin.

Scenariusze:

- **A:** Rust ogłasza blok, Kubo odnajduje jego `PeerId`.
- **B:** Kubo ogłasza blok, Rust odnajduje `PeerId` Kubo.
- **C:** Drugi Rust odnajduje pierwszego Rusta przez sieć DHT z Kubo.

Test Rust znajduje się w `test-runner/tests/interoperability/dht_interop.rs` i wymaga `ipfs` (Kubo) w `PATH`. Skrypt używa tymczasowego repozytorium Kubo, a dla testów `127.0.0.1` ustawia `Routing.LoopbackAddressesOnLanDHT=true`. Konfiguracja `Routing.Type=custom` i `Routing.Routers` jest oznaczona w Kubo jako **eksperymentalna** i może wymagać dostosowania do konkretnej wersji Kubo.

### 9.4. Regresja Etapu 4

```bash
cargo test -p test-runner --test integration-network -- --test-threads=1
```

Zmiana `node start` bez argumentów wymagała aktualizacji starszego testu, który wcześniej traktował taki start jako niepoprawny. Inne mechanizmy M4 powinny pozostać zachowane.

## 10. Definition of Done i status

| Kryterium | Stan (12.11.2026) |
|---|---|
| Kademlia w istniejącym `NetworkBehaviour` | **Zrobione** |
| Bootstrap i zdarzenia tablic routingu | **Zrobione** |
| Client domyślny / Server flagą | **Zrobione** |
| Provider discovery po multihash | **Zrobione** |
| Testy jednostkowe DHT (`dht-unit-test`) | **PASS** |
| Wiele węzłów Rust (scenariusz C) | **PASS** |
| Interoperacyjność z Kubo (`interop-dht`, scenariusze A, B, C) | **PASS** |
| Regresja M4 (`integration-network`, `interop-network`) | **PASS** |
| Rzeczywiste pobieranie bloków od zdalnego peera | Poza M5, planowane w M6/M7 (Bitswap) |

Issues #6 i #7 zostały zamknięte. Przy cytowaniu wyników w pracy należy podać hash commita (`git rev-parse HEAD`) i wersję Kubo (`ipfs version`).

## 11. Ograniczenia i ryzyka

1. **`MemoryStore`:** rekordy lokalne nie przeżywają restartu procesu. Obecna wersja nie wykonuje automatycznego `announce_all` po enumeracji Blockstore. Po restarcie ponownie wywołujemy `provide`.
2. **DHT nie oznacza transferu:** wyszukanie `PeerId` nie gwarantuje posiadania bloku ani pobrania go. Bitswap dopiero M6/M7.
3. **Brak mDNS:** bez bootstrapu pierwszy serwer sam nie skanuje urządzeń w LAN.
4. **Adresy i firewall:** nasłuchiwanie na `127.0.0.1` nie wystarczy innym komputerom; wymagane są poprawne adresy LAN i otwarte porty.
5. **`stop_providing`:** wycofuje lokalne ogłaszanie, ale nie usuwa natychmiast wszystkich zapisów u obcych peerów.
6. **Metadane:** rekord providera ujawnia związek między multihashem a dostawcą. LAN nie zastępuje autoryzacji EHR ani szyfrowania danych. W demonstracji używamy fikcyjnych bloków.
7. **Wydajność:** XOR mierzy odległość logiczną, nie geograficzną. W M5 nie implementujemy polityki preferowania peerów według RTT.
8. **Tryb Server:** określa obsługę żądań DHT, a nie to, czy TCP działa dwukierunkowo. Client również ma transport libp2p.
9. **Kubo custom routing:** konfiguracja LAN w skrypcie jest przeznaczona do testów i opiera się na eksperymentalnych opcjach Kubo.
10. **Pełna topologia:** sama obecność peera w `k-bucket` nie oznacza, że aplikacja ma już bezpośrednio otwarte połączenie TCP do każdego wpisu w tablicy.

## 12. Zalecane zrzuty ekranu do pracy inżynierskiej

| Zrzut | Co pokazać | Co udowadnia |
|---|---|---|
| **S1** | A: `LOCAL_PEER_ID`, `LISTEN`, `[DHT] LAN Server`, brak bootstrapu | Pierwszy serwer sieci |
| **S2** | B: `CONNECTED <PEER_A>`, Identify, `DHT_ROUTING_UPDATED` | Dołączenie kolejnego serwera i routing |
| **S3** | C: `[DHT] LAN Client`, `CONNECTED <PEER_B>`, `peers` | Pacjent jako Client z bootstrapem do B |
| **S4** | A: `provide <CID>`; C: `DHT_PROVIDER <PEER_A>` | Provider discovery bez ręcznego bootstrapa C → A |
| **S5** | C: `DHT_PROVIDER` i osobno `[BLOCKSTORE] HAS false` | DHT znajduje dostawcę, ale nie pobiera bloku |
| **S6** | B: reconnect; A: identyczny `LOCAL_PEER_ID` po restarcie | Stabilna tożsamość i reconnect z M4 |
| **S7** | Konsola testów jednostkowych zakończonych `ok` | Poprawność lokalnej logiki |
| **S8** | Wyjście `test-kubo-dht-interop.sh` zakończone `PASS: Rust <-> Kubo LAN DHT provider discovery` | Interoperacyjność z Kubo |

W podpisach zdjęć podaj adresy prywatne i skrócone `PeerId`; nie prezentuj żadnych danych pacjenta.

## 13. Źródła

- [IPFS Docs — Distributed Hash Tables](https://docs.ipfs.tech/concepts/dht/)
- [rust-libp2p 0.57.1](https://docs.rs/crate/libp2p/0.57.1)
- [libp2p-kad — Behaviour API](https://docs.rs/libp2p-kad/0.49.0/libp2p_kad/struct.Behaviour.html)
- [Kubo — Configuration](https://github.com/ipfs/kubo/blob/master/docs/config.md)
- [Kod projektu decentralized-ehr](https://github.com/KytryshAndrii/decentralized-ehr)
