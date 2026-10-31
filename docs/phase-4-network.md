# Phase 4 — Bootstrap, Identify, Ping, Reconnect

## Cel
Issue #5: nowy węzeł łączy się z określonym bootstrap peerem i odzyskuje połączenie po jego restarcie. Węzły są równoprawne pod względem posiadania modułu IPFS. Bootstrap jest adresem wejścia, a nie bazą danych EHR ani serwerem autoryzacji.

## Architektura

```
NetworkConfig (listen, identity_path, bootstrap peers, retry policy)
    |
Identity (trwały klucz Ed25519 -> stabilny PeerId)
    |
Swarm (TCP + Noise + Yamux)
    |
NetworkBehaviour (Identify + Ping)
    |
Event loop (Connect/Disconnect/Identify/Ping/DialFailed)
    |
Retry state (per peer exponential backoff)
```

## Co robią protokoły?

- Bootstrap: połączenie z uprzednio znanym adresem `/ip4/host/tcp/port/p2p/PeerId`.
- Identify: po ustanowieniu połączenia ujawnia tożsamość peera, adresy i obsługiwane protokoły. Nie jest DHT ani discovery.
- Ping: pomiar RTT i sprawdzenie stanu połączenia. Nie utrzymuje gwarantowanej dostępności.
- Reconnect: po `ConnectionClosed` przy zerowej liczbie aktywnych połączeń albo `OutgoingConnectionError` planuje ponowny dial, z opóźnieniem 2, 4, 8, 16, 30 sekund.
- Klucz Ed25519: zapisany lokalnie, bo po restarcie PeerId musi pozostać ten sam.

## Wybory architektoniczne

- Rust `libp2p = 0.56.0`, Tokio, TCP, Noise, Yamux. Nie piszemy własnego transportu ani szyfrowania.
- Bootstrap to lista *znanych* peerów, a nie zewnętrzny serwer scentralizowany.
- DHT/Kad, peer routing, NAT traversal, relay, Bitswap i UCAN nie wchodzą do zakresu Issue #5.
- Retry z wykładniczym opóźnieniem ogranicza zalewanie połączeniami przy awarii.
- `idle_timeout` = 90 s, ping co 15 s. Ping nie powinien być mylony z retransmisją ani gwarancją połączenia.
- Listen na 127.0.0.1 dla demonstracji na jednej maszynie. Dla Fedora Server zmienić na LAN IP i ograniczyć firewall.
- Nie commitować kluczy prywatnych. Zachować osobny plik klucza na każdym węźle.

## Pliki

- `config.rs` konfiguracja, parsowanie bootstrap multiaddr.
- `identity.rs` persist/restore libp2p Ed25519.
- `behaviour.rs` Identify, Ping.
- `node.rs` transport i pętla zdarzeń / retry.
- `error.rs` błędy konfiguracyjne i runtime.
- `src/bin/ipfs-network-node.rs` program demonstrujący działanie.

## Uruchomienie, Linux

Terminal 1:

```bash
cargo run -p ipfs-network --bin ipfs-network-node -- \
  ./tmp/node-a.key.pb /ip4/127.0.0.1/tcp/4101
```

Skopiować `PeerId` z linii `LOCAL_PEER_ID`.

Terminal 2:

```bash
cargo run -p ipfs-network --bin ipfs-network-node -- \
  ./tmp/node-b.key.pb /ip4/127.0.0.1/tcp/4102 \
  /ip4/127.0.0.1/tcp/4101/p2p/PEER_ID_A
```

Wyniki oczekiwane (niezweryfikowane w niniejszym środowisku): CONNECTED, IDENTIFY, PING. Po zamknięciu A zobaczymy DISCONNECTED lub błąd dzwonienia, a po ponownym uruchomieniu A z tym samym plikiem klucza i na tym samym porcie B powinien sam odzyskać połączenie.

Ważne: oba węzły mogą inicjować połączenie. Jeśli chcesz, aby A automatycznie wracał do B, dodaj B do bootstrap A. Ponowne wybieranie przez B wystarcza do demonstracji reconnect.

## Ograniczenia i zagrożenia

1. Stabilność adresów: jeśli port lub adres bootstrap zmieni się po restarcie, statyczny dial już nie zadziała. Potrzebna aktualizacja adresu lub discovery w przyszłym etapie.
2. Stabilność tożsamości: utrata klucza prywatnego oznacza nowy PeerId.
3. `Identify` nie certyfikuje uprawnień lekarza; to warstwa autoryzacji EHR.
4. Ochrona danych EHR: nie wysyłać informacji medycznych przez sam Identify. Warstwa transportowa jest zaszyfrowana, ale nie zastępuje autoryzacji i szyfrowania danych w spoczynku.
5. Backoff gwarantuje ponawianie prób, a nie sukces, jeśli sieć pozostaje niedostępna.
6. Przy dużej liczbie peerów warto dodać losowy jitter, limity połączeń, trwałe adresy, tracing i rejestr peerów.
7. Możliwa konieczność dostosowania szczegółów API do istniejącego `Cargo.lock`; przed scaleniem kompilacja i testy na maszynie użytkownika są obowiązkowe.


## Polecenia
Polecenie	Działanie
./ehr-cli --help	Wyświetla pomoc
./ehr-cli node identity <plik>	Tworzy lub odczytuje PeerId
./ehr-cli node start <plik> <adres>	Uruchamia węzeł nasłuchujący
./ehr-cli node start <plik> <adres> <bootstrap>	Uruchamia węzeł i inicjuje połączenie


## Literatura

1. https://github.com/KytryshAndrii/ipfs_prototype/issues/5
2. https://docs.libp2p.io/concepts/discovery-routing/bootstrap/
3. https://docs.libp2p.io/concepts/fundamentals/peers/
4. https://docs.libp2p.io/concepts/fundamentals/addressing/
5. https://docs.libp2p.io/concepts/secure-comm/noise/
6. https://github.com/libp2p/specs/tree/master/identify
7. https://github.com/libp2p/specs/tree/master/ping
8. https://docs.rs/libp2p/0.56.0/libp2p/identify/index.html
9. https://docs.rs/libp2p/0.56.0/libp2p/ping/index.html
10. https://docs.rs/libp2p/0.56.0/libp2p/swarm/index.html
11. https://github.com/libp2p/rust-libp2p/tree/master/examples
12. https://docs.ipfs.tech/how-to/configure-node/#bootstrap
