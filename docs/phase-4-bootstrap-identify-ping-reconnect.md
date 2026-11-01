# Phase 4 — Bootstrap, Identify, Ping, Reconnect

**Projekt:** `decentralized-ehr`
**Zakres:** Issue #5 oraz późniejsze usprawnienia prototypu sieciowego i CLI
**Stan dokumentacji:** 1 listopada 2026 r.
**Repozytorium:** https://github.com/KytryshAndrii/decentralized-ehr
**Status:** połączenie Rust ↔ Rust na dwóch komputerach potwierdzone; wykrywanie i odzyskiwanie połączenia demonstrowane; test Rust ↔ Kubo historycznie przeszedł, lecz po zmianach CLI wymaga ponownego potwierdzenia; nie wszystkie zmiany lokalne muszą być wypchnięte do publicznej gałęzi.

> **Zasada terminologiczna:** aż do wdrożenia mechanizmów uwierzytelniania i autoryzacji (rozważane UCAN i GPG/OpenPGP) nie przypisujemy uczestnikom sieci ról „pacjent”, „lekarz” ani „placówka”. Stosujemy nazwy **Host A**, **Host B**, **node**, **peer**, **host** lub **user**. Fedora może być umownie domyślnym hostem bootstrap, a nie autorytatywnym „serwerem szpitala”.

## 1. Cel etapu i kryteria akceptacji

Issue #5 zakłada, że nowy węzeł potrafi połączyć się z wcześniej znanym peerem bootstrap, wymienić metadane Identify, zmierzyć RTT mechanizmem Ping oraz **samodzielnie odzyskać połączenie** po niedostępności i restarcie drugiego peera. Każdy węzeł posiada własną implementację modułu IPFS i trwałą tożsamość kryptograficzną. Bootstrap to punkt wejścia do sieci, a nie baza danych EHR, serwer dostępu ani urząd certyfikacji ról.

**Kryteria funkcjonalne:**

1. Wygenerowanie trwałej tożsamości Ed25519 i odtworzenie tego samego PeerId po restarcie procesu.
2. Nasłuchiwanie TCP na skonfigurowanym adresie Multiaddr.
3. Nawiązanie bezpiecznego połączenia z peerem znanym z adresu `/ip4/.../tcp/.../p2p/<PeerId>`.
4. Obserwacja `CONNECTED`, `IDENTIFY` oraz `PING` po stronie węzłów.
5. Rozpoznanie utraty połączenia, planowanie kolejnych prób Dial i ponowne połączenie po wznowieniu hosta.
6. Zachowanie prostego, czytelnego CLI i testów regresyjnych.

## 2. Stos technologiczny i architektura

```text
                      ehr-cli (interfejs terminalowy)
                                |
                        NetworkConfig
           (listen, identity_path, bootstrap, czasy)
                                |
               identity.rs: trwały klucz Ed25519
                                |
                       libp2p Swarm
                   TCP + Noise + Yamux
                                |
                     NetworkBehaviour
                      Identify + Ping
                                |
                         Event loop
     NewListenAddr / ConnectionEstablished / ConnectionClosed
     OutgoingConnectionError / Identify / Ping / Ctrl+C
                                |
                RetryState dla znanych peerów
                  HashMap<PeerId, RetryState>
                                |
               Exponential backoff / reconnect
```

**Wersje:** przekazany pierwotny opis etapu podawał `libp2p = 0.56.0`, ale publiczny `crates/ipfs-network/Cargo.toml` sprawdzony 1 listopada 2026 r. deklaruje **`libp2p = 0.57.1`**. Dla konkretnego wydania pracy należy przyjąć wersję z właściwego commita i `Cargo.lock`, a nie starszą wartość z notatek. Tokio obsługuje asynchroniczną pętlę zdarzeń. Projekt nie implementuje własnego szyfrowania transportowego.

**Protokoły i komponenty:**

| Komponent | Zadanie | Czego nie zapewnia |
|---|---|---|
| TCP | Dwukierunkowy transport danych między hostami | Samodzielnego odkrywania peerów |
| Noise | Uwierzytelnienie tożsamości peera na poziomie transportu i szyfrowanie połączenia | Uprawnień medycznych lub szyfrowania danych na dysku |
| Yamux | Multipleksowanie strumieni w ramach istniejącego połączenia | Protokołu wymiany plików |
| Identify | Udostępnianie identyfikatora, adresów, agenta i obsługiwanych protokołów | DHT i autoryzacji |
| Ping | Pomiar RTT i sygnalizacja stanu łączności | Gwarancji dostępności lub retransmisji plików |
| Bootstrap | Startowe połączenie z uprzednio znanym peerem | Globalnego discovery |
| Retry / reconnect | Próby przywrócenia połączenia przy awarii | Gwarancji, że zdalny węzeł będzie osiągalny |

### 2.1. Listen i Dial nie są rolami węzła

`Swarm::listen_on()` uruchamia nasłuchiwanie, natomiast `Swarm::dial()` inicjuje połączenie. **Jeden i ten sam host może wykonywać obie operacje**, a po ustanowieniu połączenia oba końce mogą otwierać strumienie i inicjować wymianę komunikatów. Kierunek zestawienia TCP nie określa kierunku przyszłych transferów.

W demonstracji z dwoma hostami skonfigurowano także połączenia wzajemne: Host A znał adres Host B, a Host B znał domyślny bootstrap Host A. Może to prowadzić do równoczesnych prób Dial, ale nie wymaga dwóch oddzielnych kanałów transportowych. **Przesyłanie bloków/pliku przez sieć nie zostało tym etapem zaimplementowane**: przyszły Bitswap lub inny protokół będzie obsługiwał transmisję na istniejących połączeniach.

## 3. Struktura kodu

| Plik | Odpowiedzialność |
|---|---|
| `crates/ipfs-network/src/models.rs` | `BootstrapPeer`, `NetworkConfig`, `RetryState`, `NetworkNode` |
| `crates/ipfs-network/src/consts.rs` | Centralnie zdefiniowane domyślne wartości (w lokalnych zmianach) |
| `crates/ipfs-network/src/config.rs` | Parsowanie bootstrap Multiaddr, wartości domyślne konfiguracji |
| `crates/ipfs-network/src/identity.rs` | Odczyt/utworzenie trwałego klucza Ed25519 |
| `crates/ipfs-network/src/behaviour.rs` | Identify i Ping, mapowanie zdarzeń `NetworkEvent` |
| `crates/ipfs-network/src/node.rs` | Budowa Swarm, Listen, Dial, obsługa zdarzeń, reconnect |
| `crates/ipfs-network/src/error.rs` | Typy błędów sieciowych |
| `crates/ipfs-network/src/lib.rs` | Publiczne API biblioteki |
| `apps/ehr-cli/src/main.rs` | Interpretacja poleceń i prezentacja zdarzeń |
| `test-runner/tests/integration/network_integration.rs` | Testy procesowe Rust ↔ Rust |
| `test-runner/tests/interoperability/network_interop.rs` | Uruchomienie testu Rust ↔ Kubo |
| `scripts/testing/test-network.sh` | Lokalny scenariusz Kubo i Rust |

**Zmiana wobec pierwszej dokumentacji:** początkowo testy pokazywano także przez `src/bin/ipfs-network-node.rs` i jawne pliki kluczy. Obecny preferowany interfejs demonstracyjny to `ehr-cli` z automatyczną tożsamością. Nie należy przedstawiać starej składni jako obowiązującej.

## 4. Decyzje architektoniczne

### 4.1. Trwała, lokalna tożsamość

- Klucz Ed25519 powstaje przy pierwszym uruchomieniu i jest ponownie odczytywany w kolejnych procesach.
- `PeerId` wynika z publicznego klucza; utrata lub wymiana pliku klucza powoduje zmianę PeerId.
- Wprowadzono standardową ścieżkę **`./tmp/node-key.pb`**, określoną stałą `DEFAULT_IDENTITY_PATH` w `ipfs-network/src/consts.rs` (lokalna zmiana).
- Użytkownik **nie podaje ścieżki klucza** przy `node identity` ani `node start`.
- Plik jest prywatny, z uprawnieniami ograniczonymi na systemach Unix. `./tmp/` trzeba wykluczyć z repozytorium Git.
- Ścieżka zaczynająca się od `./` zależy od **bieżącego katalogu roboczego**, a nie automatycznie od położenia binarki. Na różnych komputerach należy uruchamiać CLI z przewidywalnego katalogu.
- Każdy host ma własny `./tmp/node-key.pb` we własnym środowisku. Nie kopiujemy jednego klucza między hostami.

**Migracja z dawnych nazw** (`hospital.key.pb`, `patient.key.pb`): na danym hoście można przenieść jego wcześniejszy klucz do `./tmp/node-key.pb`, by zachować PeerId. Nie należy nadpisywać istniejącego pliku bez wcześniejszego sprawdzenia.

### 4.2. Trzy tryby uruchamiania

Świadomie **odrzucono** proponowany wcześniej czwarty tryb łączący równocześnie `--default-bootstrap` i `--bootstrap`. Dopuszczalne są dokładnie trzy warianty:

| Tryb | Składnia | Znaczenie |
|---|---|---|
| Listen-only | `ehr-cli node start <listen>` | Host nasłuchuje, nie inicjuje automatycznie Dial do znanego peera |
| Default bootstrap | `ehr-cli node start <listen> --default-bootstrap` | Host nasłuchuje i próbuje połączyć się z domyślnym peerem |
| Custom bootstrap | `ehr-cli node start <listen> --bootstrap <multiaddr/p2p/PeerId>` | Host nasłuchuje i próbuje połączyć się z jednym jawnie wskazanym peerem |

- Tryby bootstrap są **wzajemnie wykluczające się**.
- W bieżącej wersji CLI zachowano także obsługę **starego pozycyjnego Multiaddr** jako alias trybu custom dla zgodności wstecznej testów, ale nie jest to dodatkowy tryb.
- Po wdrożeniu Kademlia DHT możliwe będzie odkrywanie i obsługa wielu peerów; nie jest to jednak obecnie włączone.
- Wybranie domyślnego bootstrapu na samym hoście bootstrap nie powinno prowadzić do Dial do własnego PeerId; konstruktor `NetworkNode` ignoruje taki wpis.

### 4.3. Domyślny bootstrap

W lokalnej konfiguracji przyjęto Fedorę jako **znany punkt kontaktowy**:

```text
/ip4/192.168.1.125/tcp/4101/p2p/<PEER_ID_HOST_A>
```

Wartości adresu, portu i PeerId są umieszczone w `ipfs-network/src/consts.rs`, a nie rozrzucone po CLI. Własny bootstrap można wskazać opcją `--bootstrap`. PeerId wymaga dokładnego przepisania z `LOCAL_PEER_ID`; adres i identyfikator mogą się zmieniać przy zmianie klucza lub konfiguracji DHCP. Stały bootstrap nie jest pojedynczym źródłem prawdy o EHR.

**Uwaga bezpieczeństwa:** wpisanie adresu `192.168.1.125` działa w konkretnej sieci testowej; nie jest rozwiązaniem odkrywania globalnego ani mechanizmem konfiguracji produkcyjnej.

### 4.4. Parametry czasowe

Zgodnie z ustaleniami prototypu:

| Parametr | Wartość |
|---|---|
| Początkowy retry | 2 s |
| Maksymalny retry | 30 s |
| Maintenance tick | 1 s |
| Interwał Ping | 15 s |
| `idle_connection_timeout` | 90 s |

Backoff przechodzi typowo przez **2, 4, 8, 16, 30 s**, po czym utrzymuje limit 30 s. Po ustanowieniu połączenia opóźnienie jest resetowane. Zdarzenie `ConnectionClosed` planuje reconnect tylko wtedy, gdy nie pozostało żadne aktywne połączenie z peerem (`num_established == 0`). W produkcji przydatny będzie losowy jitter i limity prób.

### 4.5. Jawny DialOpts i kontrola stanu

W początkowym wariancie `attempt_connections()` odtwarzał Multiaddr z `/p2p/<PeerId>` i wywoływał `swarm.dial(addr)`. Zmieniono go na jawne:

```rust
let opts = DialOpts::peer_id(peer_id)
    .addresses(vec![address])
    .build();
self.swarm.dial(opts)?;
```

Wartości `peer_id` oraz transportowego `address` są wyodrębniane przez `BootstrapPeer::parse()`. Wywołanie identyfikuje oczekiwanego peera niezależnie od samego adresu transportowego.

Po testach z jednoczesnym bootstrapem w obu kierunkach zaproponowano dodatkową kontrolę:

```rust
if self.swarm.is_connected(&peer_id) {
    state.connected = true;
    state.dialing = false;
    continue;
}
```

Jej celem jest pomijanie zbędnych prób Dial, kiedy połączenie już istnieje. **Nie zmienia to dwukierunkowości komunikacji.** Nie usuwa jednak każdego możliwego wyścigu przy równoczesnym Dial. `PeerCondition::NotDialing` może oznaczać, że libp2p odmówił redundatnej próby, ponieważ trwa już inne nawiązywanie połączenia lub peer jest połączony. W logowaniu warto odróżnić „pominięto nadmiarową próbę” od rzeczywistej awarii transportu.

**Stan weryfikacji:** publiczne `node.rs` sprawdzone 1 listopada nie zawiera jeszcze powyższego warunku `is_connected()`; był on dodawany i testowany lokalnie. Podczas finalizacji rozdziału sprawdzić używany commit.

## 5. Środowisko testowe na dwóch komputerach

| Element | Host A | Host B |
|---|---|---|
| System | Fedora Server (urządzenie fizyczne) | Ubuntu 24.04 wewnątrz WSL2/Windows |
| IP węzła | `192.168.1.125/24` | `172.25.151.102/20` (przykładowy stan WSL2) |
| Adres hosta Windows | nie dotyczy | `192.168.1.51/24` |
| Router | `192.168.1.1` | `192.168.1.1` po stronie Windows |
| Port libp2p | `4101/tcp` | `4102/tcp` |
| Klucz | lokalny `./tmp/node-key.pb` | własny `./tmp/node-key.pb` |
| Typowe zadanie w demonstracji | domyślny bootstrap / Listen | Listen / Default bootstrap |

```text
                        Router 192.168.1.1
                              |
                  +-----------+-----------+
                  |                       |
       Fedora Server / Host A        Windows 192.168.1.51
          192.168.1.125                    |
          TCP 4101                   WSL2 NAT
                  |                       |
                  |               Ubuntu / Host B
                  |              172.25.151.102
                  |                 TCP 4102
                  +---- komunikacja P2P ---+
```

### 5.1. Znaczenie różnych podsieci

WSL2 z domyślnym NAT ma adres z podsieci `172.25...`, niezależny od podsieci LAN `192.168.1.0/24`. **To normalne**: ruch wychodzący z WSL2 do Fedory jest tłumaczony przez Windows. Różne maski podsieci nie były przyczyną początkowej odmowy uwierzytelnienia PeerId.

### 5.2. Firewall Fedora

Używana strefa firewalld: `FedoraServer`. Port TCP `4101` można ograniczyć do lokalnej podsieci:

```bash
sudo firewall-cmd --get-active-zones
sudo firewall-cmd --zone=FedoraServer \
  --add-rich-rule='rule family="ipv4" source address="192.168.1.0/24" port port="4101" protocol="tcp" accept'
sudo firewall-cmd --zone=FedoraServer --list-all
```

Komendy bez `--permanent` dodają reguły **tymczasowe**. Dla dokładnego audytu trzeba także sprawdzić, czy inne usługi firewalld (np. `ipfs`) nie otwierają tego portu szerszej grupie źródeł.

Weryfikacja TCP z WSL2:

```bash
nc -vz -w 3 192.168.1.125 4101
```

W historycznym teście zwróciło to `succeeded`, czyli ścieżka sieciowa działała, zanim poprawiono błąd PeerId.

### 5.3. Port forwarding Windows → WSL2

Aby Host A mógł **sam inicjować nowe połączenie** do WSL2, port `4102/tcp` musi być dostępny z LAN przez adres Windows `192.168.1.51`. Uruchomić PowerShell **jako administrator** i dostosować `connectaddress` do bieżącego IPv4 WSL2:

```powershell
netsh interface portproxy add v4tov4 `
  listenaddress=192.168.1.51 `
  listenport=4102 `
  connectaddress=172.25.151.102 `
  connectport=4102

New-NetFirewallRule `
  -DisplayName "EHR WSL2 TCP 4102" `
  -Direction Inbound `
  -Protocol TCP `
  -LocalPort 4102 `
  -RemoteAddress 192.168.1.125 `
  -Action Allow

netsh interface portproxy show v4tov4
```

Proces WSL2 musi nasłuchiwać na **`/ip4/0.0.0.0/tcp/4102`**, a nie jedynie `127.0.0.1`. Fedora łączy się w takim scenariuszu z **`192.168.1.51:4102`**, nie z adresem prywatnym WSL2. Sprawdzenie:

```bash
nc -vz -w 3 192.168.1.51 4102
```

Wymaga to aktualizacji reguły `portproxy`, jeśli adres WSL2 zmieni się po restarcie. Portproxy nie oznacza wdrożenia pełnego NAT traversal ani automatycznego discovery.

## 6. Aktualne CLI i scenariusze demonstracyjne

**Istotna zmiana względem oryginalnego opisu:** obecne `ehr-cli` automatycznie tworzy/odczytuje `./tmp/node-key.pb`. Nie podaje się już pliku tożsamości w argumentach.

| Polecenie | Znaczenie |
|---|---|
| `./ehr-cli help` albo `./ehr-cli --help` | Lista dostępnych komend |
| `./ehr-cli node identity` | Tworzenie/odczyt PeerId |
| `./ehr-cli node start <listen>` | Listen-only |
| `./ehr-cli node start <listen> --default-bootstrap` | Listen + Dial do domyślnego bootstrapu |
| `./ehr-cli node start <listen> --bootstrap <addr>` | Listen + Dial do własnego hosta |
| `./ehr-cli cid <absolute-file-path>` | Wyznaczenie CID danych lokalnego pliku |
| `./ehr-cli put <absolute-file-path>` | Zapis lokalnego bloku do Blockstore |
| `./ehr-cli has <CID>` | Sprawdzenie obecności bloku lokalnie |
| `./ehr-cli get <CID> <absolute-output-path>` | Odtworzenie danych lokalnego bloku do pliku |

Ostatnie cztery polecenia to równoległa demonstracja integracji `ipfs-cid` i `ipfs-store` z serwisem `IpfsService`; **nie są protokołem transmisji przez sieć**.

### 6.1. Test A — WSL inicjuje połączenie do Fedory

Na Fedora Server (Host A):

```bash
cd ~/decentralized-ehr
./ehr-cli node identity
./ehr-cli node start /ip4/192.168.1.125/tcp/4101
```

Na WSL2 (Host B):

```bash
cd ~/ipfs_prototype
./ehr-cli node identity
./ehr-cli node start /ip4/0.0.0.0/tcp/4102 --default-bootstrap
```

Przykładowe klasy logów: `LOCAL_PEER_ID`, `LISTEN`, `DIAL EXPECTED`, `DIAL ADDRESS`, `CONNECTED`, `IDENTIFY`, `PING`.

### 6.2. Test B — Fedora inicjuje połączenie do WSL2

Po skonfigurowaniu Windows portproxy na WSL2 uruchomić Listen-only:

```bash
./ehr-cli node start /ip4/0.0.0.0/tcp/4102
```

Na Fedorze:

```bash
./ehr-cli node start /ip4/192.168.1.125/tcp/4101 \
  --bootstrap "/ip4/192.168.1.51/tcp/4102/p2p/<PEER_ID_HOST_B>"
```

W obu scenariuszach systemy mogą inicjować wymianę informacji przez połączenie, bez narzucania „trybu serwera” na poziomie TCP.

### 6.3. Test C — rozłączenie i ponowne połączenie

1. Ustanowić połączenie; poczekać na `CONNECTED`, `IDENTIFY`, `PING`.
2. Zatrzymać Host A przez `Ctrl+C`.
3. Na Host B obserwować `DISCONNECTED` lub nieudane Dial i kolejne opóźnienia retry.
4. Ponownie uruchomić Host A na tym samym porcie, z **tym samym kluczem** `./tmp/node-key.pb`.
5. Bez restartu Host B obserwować nowe `CONNECTED`, `IDENTIFY` i `PING`.

Ten scenariusz jest właściwym dowodem reconnect; samo `CONNECTED` po błędzie przy pierwszym uruchomieniu nie dowodzi jeszcze reconnect po utracie wcześniej ustanowionego połączenia.

### 6.4. Test D — jednoczesne Dial po obu stronach

Obydwa hosty uruchamiano z aktywnym nasłuchiwaniem oraz własnym źródłem bootstrapu. Zanotowano sytuację, gdy pojawiły się prawidłowe `CONNECTED`, `IDENTIFY`, `PING`, ale również dodatkowe komunikaty `DIAL_FAILED` i `PeerCondition::NotDialing`. Oznacza to konieczność rozróżnienia błędów transportu od niepotrzebnych, równoległych prób połączenia.

**Podpis proponowany do pracy:** „Dwukierunkowa wymiana komunikatów pomiędzy autonomicznymi hostami libp2p, niezależnie od kierunku inicjalizacji połączenia TCP”. Nie pisać „dwukierunkowy transfer plików”, dopóki moduł wymiany bloków nie zostanie zaimplementowany i przetestowany.

## 7. Diagnostyka problemów i przyjęte rozwiązania

### 7.1. `Unexpected peer ID` / `WrongPeerId` mimo poprawnego TCP

**Objawy:** TCP do `192.168.1.125:4101` działał (`nc` zwracał sukces), a klient libp2p zgłaszał błędny PeerId i ponawiał połączenie. Fedora mogła okresowo pokazywać ustanowienie/zerwanie połączenia.

**Weryfikacja:** w obsłudze `DialError::WrongPeerId` dodano diagnostykę pól `Expected`, `Obtained`, `Match`, `Address`. `Match: false` dowiódł niezgodności **rzeczywistych wartości** PeerId. Identyfikatory różniły się bardzo podobnymi wizualnie znakami: **wielkie `S` oraz cyfra `5`** w kodowaniu Base58btc.

**Rozwiązanie:** użycie dokładnego `LOCAL_PEER_ID` z Hosta A w Multiaddr drugiego hosta, zamiast ręcznego przepisywania ciągu. Jednocześnie wprowadzono jawne `DialOpts::peer_id(peer_id).addresses(...)`, aby oczekiwana tożsamość była zadeklarowana wprost.

**Wniosek:** Noise/libp2p prawidłowo odrzuciły niezgodną tożsamość. Nie należało obniżać kontroli uwierzytelniania ani zmieniać maski sieci WSL2.

### 7.2. Odmienne adresy IP i maski WSL2/Fedora

**Objawy:** WSL2 `172.25.151.102/20`, Fedora `192.168.1.125/24`.

**Wniosek:** WSL2 pracuje w osobnej sieci wirtualnej NAT, ale może nawiązywać połączenia wychodzące do LAN przez Windows `192.168.1.51`. Różne podsieci nie były błędem konfiguracji. Połączenia przychodzące **do WSL2** wymagają dodatkowego dostępu, np. Windows TCP portproxy.

### 7.3. `Connection refused` i `Connection timed out`

**Objawy:** przy wzajemnym Dial pojawiały się komunikaty o odmowie połączenia lub przekroczeniu czasu.

**Wyjaśnienie:** drugi host mógł jeszcze nie nasłuchiwać, adres/portforwarding mógł nie być gotowy albo ruch nie docierał do celu. Jeżeli po kolejnych próbach pojawia się `CONNECTED`, retry wykonał swoje zadanie. Nie każdy `DIAL_FAILED` oznacza awarię całego systemu.

### 7.4. `PeerCondition::NotDialing` po ustanowieniu połączenia

**Objawy:** Fedora pokazywała `CONNECTED` i `PING`, a także błędy kolejnych prób Dial do tego samego peera.

**Wyjaśnienie:** węzeł próbował ponownie wywołać Dial mimo aktywnego połączenia lub Dial w toku; zdarzenie nie musi oznaczać utraty łączności. Zaproponowano `self.swarm.is_connected(&peer_id)` w `attempt_connections()`, aby ograniczyć nadmiarowe żądania. Dla pełnej odporności przy równoczesnych startach warto dodatkowo analizować `DialError::DialPeerConditionFalse` / wyścigi stanu oraz poprawnie oznaczać połączenia przychodzące.

### 7.5. `test-runner` nie znajdował `ehr-cli`

**Objawy:** cztery testy procesowe kończyły się natychmiast na asercji „Build the CLI first: cargo build -p ehr-cli”, bez uruchomienia węzłów.

**Przyczyna:** testy zakładały istnienie pliku binarnego w określonym katalogu `target/debug/ehr-cli`; sam `cargo test -p test-runner` nie gwarantuje zbudowania binarki innego pakietu.

**Rozwiązanie operacyjne:** najpierw `cargo build -p ehr-cli`, następnie `cargo test -p test-runner --test integration-network ...`. Przy niestandardowym katalogu `target` można przekazać absolutną ścieżkę przez `EHR_CLI_BIN`. Nie był to błąd logiczny Identify, Ping ani reconnect.

### 7.6. Stara składnia `node identity <plik>` i `node start <plik> ...`

**Objawy:** po zmianie CLI część przykładów i skryptów wciąż przekazywała plik tożsamości jako argument.

**Decyzja:** uprościć CLI do `node identity` i `node start <listen> ...`, a `./tmp/node-key.pb` obsługiwać automatycznie. Testy uruchamiające równolegle różne hosty powinny mieć **oddzielne katalogi robocze**, aby standardowe `./tmp/node-key.pb` nie oznaczało tego samego pliku dla kilku procesów na tej samej maszynie.

### 7.7. `test rust_kubo_identify_ping_interoperability ... ignored`

**Objawy:** raport `0 passed; 1 ignored`.

**Przyczyna:** atrybut `#[ignore]` w wcześniejszej wersji testu. Nie jest to wynik potwierdzający ani zaprzeczający interoperacyjności.

**Decyzja:** usunąć `#[ignore]` i uruchamiać test bez `--ignored`. Test pozostaje zależny od dostępności programu `ipfs` (Kubo), testowego repozytorium oraz poprawnego skryptu. Jeżeli test wypisuje `SKIP`, ale harness zgłasza `ok`, należy jasno zaznaczyć, że **wtedy interoperacyjności nie sprawdzono**.

### 7.8. Rust ↔ Kubo: nowy test kończył się po ok. 0,34 s

**Objawy:** `Rust <-> Kubo test failed: exit status: 1`.

**Wstępna diagnoza:** skrypt `scripts/testing/test-network.sh` lub użyta binarka mogły nadal stosować starą składnię CLI. Zaproponowano aktualizację wywołania na:

```bash
"$CLI" node start /ip4/127.0.0.1/tcp/0 \
  --bootstrap "${KUBO_ADDR}/p2p/${KUBO_ID}"
```

oraz izolację katalogu roboczego, kontrolę poprawnej binarki i lepsze wypisywanie logów przy błędzie.

**Status:** po dostarczeniu poprawki skryptu nie przedstawiono jeszcze nowego wyniku testu Rust ↔ Kubo; **nie należy deklarować naprawy jako potwierdzonej**. Dodatkowa uwaga: test zakończony po 0,34 s nie dowodzi sam w sobie przyczyny, tylko wskazuje na bardzo wczesną awarię uruchomienia/scenariusza.

### 7.9. Niezgodność publicznej gałęzi z lokalnymi zmianami

Weryfikacja publicznego repozytorium 1 listopada 2026 r. wykazała, że:

- `apps/ehr-cli/src/main.rs` opisuje już trzy nowe tryby i automatyczny `./tmp/node-key.pb`;
- `crates/ipfs-network/Cargo.toml` wskazuje `libp2p 0.57.1`;
- publiczne `test-runner/tests/integration/network_integration.rs` nadal zawierało argumenty klucza w testach, więc **nie było jeszcze dopasowane do nowego CLI**;
- publiczne `crates/ipfs-network/src/node.rs` nie zawierało jeszcze lokalnej kontroli `is_connected()`;
- publicznego `src/consts.rs` nie udało się otworzyć pod sprawdzanym adresem (404), mimo że pojawiał się w lokalnym drzewie projektu.

Z tego powodu dokumentacja rozróżnia *decyzję projektową*, *kod lokalny* oraz *potwierdzony wynik testu*. Przed podpisaniem rozdziału należy wypchnąć i odnotować właściwy commit.

## 8. Testy i dowody

### 8.1. Rust ↔ Rust, procesowe testy integracyjne

Zestaw czterech scenariuszy:

1. `identity_persists_between_processes` — ten sam klucz zachowuje PeerId po restartach.
2. `invalid_cli_arguments_fail` — błędne argumenty powodują odpowiedni kod wyjścia.
3. `two_rust_nodes_connect_identify_and_ping` — dwa procesy nawiązują połączenie i realizują Identify/Ping.
4. `rust_reconnects_after_peer_restart` — wznowienie połączenia po restarcie peera.

**Historyczny wynik przed refaktoryzacją CLI:** 4/4 zaliczone. **W późniejszej iteracji:** wszystkie cztery testy przerwały się na wykrywaniu binarki `ehr-cli`, przed wykonaniem scenariuszy. Po zmianie składni CLI testy trzeba ponowić i udokumentować ich wynik osobno. Nie należy automatycznie przenosić historycznego 4/4 na późniejszy commit.

### 8.2. Rust ↔ Kubo

Historycznie na localhost potwierdzono komunikację TCP/Noise/Yamux/Identify/Ping z Kubo, a test zakończył się `1 passed` z obserwowanymi odpowiedziami Ping. Po refaktoryzacji CLI odnotowano najpierw `ignored`, następnie błąd uruchomienia skryptu (`exit status: 1`). Po poprawkach należy ponowić test w aktualnej wersji i wykazać rzeczywiste `CONNECTED`/`IDENTIFY`/`PING`.

### 8.3. Rzeczywiste testy LAN

Na Fedora Server i WSL2/Windows:

- potwierdzono dostępność `192.168.1.125:4101` przez `nc`;
- początkowo wykryto i poprawiono błąd PeerId (`S` kontra `5`);
- uzyskano `CONNECTED`, `IDENTIFY`, `PING` między dwoma fizycznymi komputerami (z hostem Windows/WSL2 na jednym z nich);
- przetestowano połączenia inicjowane w obie strony, po odpowiedniej konfiguracji portproxy;
- obserwowano nieudane próby Dial i skuteczne ponowne nawiązywanie połączenia;
- wykonano zrzuty ekranu uruchomienia, utraty połączenia i ponownego połączenia.

Dla pracy dyplomowej przechować zrzuty wraz z datą, opisem hosta i wersją programu. Nie ujawniać prywatnych plików kluczy.

### 8.4. Polecenia regresji

```bash
# Z katalogu głównego repozytorium
cargo fmt --all -- --check
cargo build -p ehr-cli
cargo test -p ipfs-network

cargo test -p test-runner \
  --test integration-network -- --test-threads=1 --nocapture

cargo test -p test-runner \
  --test interop-network -- --nocapture

cargo clippy --workspace --all-targets -- -D warnings
```

Jeżeli test integracyjny nie odnajduje binarki przy innym `target`:

```bash
EHR_CLI_BIN="$(realpath target/debug/ehr-cli)" \
  cargo test -p test-runner --test integration-network \
  -- --test-threads=1 --nocapture
```

W repozytorium nazwy targetów należy potwierdzić w `test-runner/Cargo.toml`; przykłady używają tych, które były uruchamiane w naszej sesji (`integration-network`, `interop-network`).

## 9. Ograniczenia, bezpieczeństwo i zakres przyszłych etapów

1. **Brak DHT/Kademlia:** bootstrap zna statyczny adres peera; nie istnieje jeszcze samoczynne odkrywanie sieci.
2. **Brak Bitswap:** Identify i Ping nie przesyłają dokumentacji ani bloków EHR.
3. **Brak NAT traversal i relay:** WSL2 wymaga przekierowania portu albo innego rozwiązania dostępności przychodzącej.
4. **Brak ról i autoryzacji EHR:** Ed25519 i Noise identyfikują peera kryptograficznie, ale nie określają, czy jest lekarzem, pacjentem ani jakie ma prawa.
5. **UCAN i GPG/OpenPGP:** rozważane jako mechanizmy przyszłego modelu delegowania uprawnień, podpisów i wymiany zaufania; nie zostały w Phase 4 zintegrowane ani ostatecznie wybrane.
6. **Szyfrowanie w spoczynku:** Noise chroni połączenie, ale nie szyfruje automatycznie lokalnych plików EHR.
7. **Stały bootstrap:** adres `192.168.1.125` zależy od prywatnej sieci testowej, a PeerId od trwałego klucza. Należy zadbać o stabilne adresy lub przyszły discovery.
8. **Timeout i backoff:** retry nie gwarantuje sukcesu; możliwe są wyścigi przy jednoczesnym Dial, retransmisje prób i czasowa niedostępność hosta.
9. **Widoczność logów:** szczegółowe `eprintln!/println!` pomagają przy diagnostyce prototypu, ale docelowo potrzebne będą `tracing`, poziomy logowania i rozróżnienie awarii od pominiętej nadmiarowej próby.
10. **Zgodność implementacji z dokumentacją:** końcowe dowody muszą dotyczyć określonego commita, jego `Cargo.lock` i faktycznie wykonanych testów.

## 10. Konkluzja do pracy dyplomowej

W Phase 4 zaimplementowano bazową warstwę sieciową opartą na Rust libp2p: trwałą tożsamość Ed25519, bezpieczny transport TCP/Noise z multipleksowaniem Yamux, protokoły Identify i Ping oraz mechanizm utrzymania i ponawiania połączeń z wykorzystaniem wykładniczego opóźnienia. Węzły są funkcjonalnie równorzędnymi uczestnikami sieci P2P; wybór bootstrapu nie nadaje uprawnień aplikacyjnych ani nie tworzy centralnego serwera EHR. Testy na osobnych komputerach, z których jeden pracował w WSL2 za NAT, potwierdziły możliwość ustanowienia połączenia, wymiany komunikatów i diagnostyki problemów tożsamości i dostępności transportowej. Dalszy rozwój obejmuje Kademlia DHT, wymianę bloków przez Bitswap, zarządzanie dostępnością hostów i docelowy model uwierzytelniania oraz autoryzacji.

## 11. Literatura i materiały

1. Issue #5 (historyczny adres, sprawdzić ewentualne przeniesienie): https://github.com/KytryshAndrii/ipfs_prototype/issues/5
2. Repozytorium projektu: https://github.com/KytryshAndrii/decentralized-ehr
3. libp2p Bootstrap: https://docs.libp2p.io/concepts/discovery-routing/bootstrap/
4. libp2p Peers: https://docs.libp2p.io/concepts/fundamentals/peers/
5. libp2p Addressing: https://docs.libp2p.io/concepts/fundamentals/addressing/
6. libp2p Noise: https://docs.libp2p.io/concepts/secure-comm/noise/
7. Identify specification: https://github.com/libp2p/specs/tree/master/identify
8. Ping specification: https://github.com/libp2p/specs/tree/master/ping
9. Identify API, wersja historyczna: https://docs.rs/libp2p/0.56.0/libp2p/identify/index.html
10. Ping API, wersja historyczna: https://docs.rs/libp2p/0.56.0/libp2p/ping/index.html
11. Swarm API, wersja historyczna: https://docs.rs/libp2p/0.56.0/libp2p/swarm/index.html
12. rust-libp2p examples: https://github.com/libp2p/rust-libp2p/tree/master/examples
13. Kubo/IPFS bootstrap: https://docs.ipfs.tech/how-to/configure-node/#bootstrap
14. libp2p 0.57.1 (wersja zadeklarowana publicznie): https://docs.rs/libp2p/0.57.1/libp2p/

---

**Przed oddaniem:** zapisać hash commita (`git rev-parse HEAD`), wynik `cargo test`, wersje narzędzi oraz podpisy wykonanych screenshotów. Oznaczyć testy Rust ↔ Kubo jako zaliczone dopiero po ponownym poprawnym wykonaniu na finalnym commicie.
