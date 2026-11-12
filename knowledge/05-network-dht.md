# 05 — Sieć: M4 (bootstrap/reconnect) i M5 (Kademlia DHT)

## 1. Stos

```text
ehr-cli ── NetworkConfig ── identity.rs (Ed25519, ./tmp/node-key.pb → stabilny PeerId)
                 │
           libp2p Swarm: TCP + Noise + Yamux   (jeden Swarm na proces)
                 │
           NetworkBehaviour { identify, ping, kad }   (+ bitswap w M6/M7)
                 │
           NetworkNode::run_inner — tokio::select!{ tick(1s) | swarm event | NetworkCommand | Ctrl+C }
                 │
           RetryState per bootstrap peer (backoff 2→30 s)
```

- **Listen i Dial to nie role.** Każdy host może jedno i drugie. Po zestawieniu połączenia oba końce otwierają strumienie (Yamux). Kierunek TCP nie ogranicza przyszłych transferów (np. Bitswap w obie strony po jednym połączeniu).
- **Bootstrap** to punkt wejścia (znany adres `/ip4/../tcp/../p2p/<PeerId>`), a nie centralny serwer EHR ani urząd autoryzacji.
- **Identify** podaje PeerId, adresy i protokoły; nie jest discovery ani autoryzacją. **Ping** mierzy RTT; nie gwarantuje dostępności.
- **Noise** uwierzytelnia PeerId na poziomie transportu i szyfruje kanał, ale nie szyfruje danych w spoczynku i nie nadaje ról.

## 2. Problemy M4 i rozwiązania (materiał do pracy)

| Problem | Przyczyna | Rozwiązanie |
|---|---|---|
| `WrongPeerId` mimo działającego TCP | Ręcznie przepisany PeerId: **`S` vs `5`** w base58 | Kopiować `LOCAL_PEER_ID`; `DialOpts::peer_id(..)` + diagnostyka Expected/Obtained/Match |
| Różne podsieci WSL2 (172.25.x/20) vs LAN (192.168.1.x/24) | WSL2 za NAT Windows | To normalne. Ruch przychodzący do WSL2 wymaga `netsh portproxy` + reguły firewall Windows; nasłuch `0.0.0.0` |
| `Connection refused` / `timed out` | Drugi host jeszcze nie słucha / brak forwardu | Retry działa; `CONNECTED` po kilku próbach jest OK |
| `PeerCondition::NotDialing` | Dial do już połączonego peera (wzajemny bootstrap) | `if swarm.is_connected(&peer_id) { continue }` (jest w kodzie) |
| test-runner nie znajduje `ehr-cli` | `cargo test` nie buduje binarki innego pakietu | najpierw `cargo build -p ehr-cli` lub `EHR_CLI_BIN=...` |
| Interop Kubo kończy się po 0,34 s | Skrypt używał starej składni (plik klucza) | Nowa składnia `node start <listen> --bootstrap <addr>`; po poprawce `interop-network` PASS |

## 3. M5 — Kademlia w kodzie

- `build_kademlia(local_id, DhtMode)`: `kad::Config::new(StreamProtocol::new("/ipfs/lan/kad/1.0.0"))`, `MemoryStore`, query timeout 30 s, `set_mode(Some(Client|Server))`.
- Bootstrap peery z configu trafiają do `kad.add_address` i do `RetryState`.
- `identify::Event::Received`: jeśli peer ogłasza LAN DHT, jego `listen_addrs` trafiają do `kad.add_address` i uruchamia się `kad.bootstrap()` (raz). **Pułapka rust-libp2p:** sam fakt połączenia nie dodaje peera do tablicy routingu; trzeba to zrobić ręcznie przez Identify.
- `NewListenAddr` z konkretnym IP trafia do `add_external_address` (potrzebne w trybie Server).
- Zdarzenia: `RoutingUpdated`, `Bootstrap` progress, `StartProviding`, `GetProviders::FoundProviders` (filtr własnego PeerId, deduplikacja w `pending_provider_queries`), `step.last` daje `DHT_FIND_FINISHED count=N`.
- `provide <CID>` w CLI najpierw sprawdza lokalny Blockstore (`IpfsService::has_block`).

## 4. Client vs Server

| | Client (pacjent, domyślnie) | Server (placówka, `--dht-server`) |
|---|---|---|
| Połączenia libp2p in/out, Ping, Identify | tak | tak |
| Wysyła zapytania DHT, `get_providers`, `start_providing` | tak | tak |
| **Odpowiada na zapytania Kademlia innych** | **nie** | tak |
| Ogłasza `/ipfs/lan/kad/1.0.0` w Identify | nie (dlatego Client nie trafia do tablic innych) | tak |
| Uprawnienia medyczne | z warstwy UCAN/GPG, nie z trybu DHT | to samo |

Tryb Server nie zapewnia osiągalności: potrzebny jest dostępny adres, nasłuch i firewall. Domyślny tryb Client w rust-libp2p nie odpowiada Kubo, dlatego węzeł testowy w interop powinien być Server.

## 5. Warunki awansu pacjenta Client → Server (PROPOSED, nie w M5)

Awans tylko po spełnieniu warunków, z histerezą (kilka udanych testów w górę, kilka nieudanych w dół):
1. **Osiągalność:** inny węzeł potrafi zainicjować połączenie przychodzące (np. AutoNAT). *Obowiązkowe.*
2. Znany osiągalny `Multiaddr`. *Obowiązkowe.*
3. Stabilność łączności, np. 10 min (wartość projektowa, nie wymóg Kademlii). *Zalecane.*
4. Zasoby: limity połączeń, CPU i pamięci nieprzekroczone. *Obowiązkowe.*
5. Brak trybu oszczędzania energii i sieci komórkowej. *Zalecane.*
6. **Zgoda użytkownika** na udział w routingu. *Obowiązkowe.*
7. Łączność z innymi serwerami DHT. *Obowiązkowe.*

Powrót do Client: utrata osiągalności lub połączenia, cofnięcie zgody, oszczędzanie energii. libp2p ma `set_mode(None)` (auto, na podstawie potwierdzonego adresu zewnętrznego), ale to nie uwzględnia zgody ani zasobów.

## 6. LAN vs WAN (zob. D-22 w `03-...`)

- `/ipfs/lan/kad/1.0.0`: DHT dla adresów prywatnych. **W kodzie używany jest wyłącznie ten.**
- `/ipfs/kad/1.0.0`: publiczny Amino WAN DHT. Kubo kwalifikuje do tablic tylko publicznie osiągalne adresy, więc na prywatnych IP test WAN może nie działać.
- **LAN ≠ „ten sam router”.** VPN warstwy 3 (routowane adresy) pozwala używać LAN DHT między miastami. Wymaga infrastruktury i kontroli dostępu. VPN ≠ VLAN.
- Do sieci rozproszonej przez Internet LAN DHT nie jest wymagany. Wystarczy wspólny protokół DHT, osiągalny bootstrap i realna możliwość połączenia libp2p.
- Własny identyfikator (np. `/ehr/kad/1.0.0`) daje logicznie wydzieloną sieć, ale nie zapewnia bezpieczeństwa, a Kubo się do niej nie dołączy.
- Kademlia nie skanuje podsieci; mDNS nie jest dodany. Pierwszy peer musi być znany (bootstrap).
- Publiczny DHT ujawnia, kto dostarcza dany multihash i kto go szuka. **Nie ogłaszać rzeczywistych EHR w publicznym DHT.**

## 7. Matematyka XOR (do pracy)

- Przestrzeń 256-bit: `K = SHA-256(PeerId bytes)`, klucz rekordu `= SHA-256(multihash)`. Odległość `d(A,B) = A ⊕ B` czytana jako liczba całkowita.
- `d = Σ 2^i (a_i ⊕ b_i)`; ponieważ `2^k > Σ_{i<k} 2^i = 2^k − 1`, **pierwszy różniący się bit dominuje**, więc dłuższy wspólny prefiks oznacza mniejszą odległość. Przykład 8-bit: T=10110010, A=10110011 → 1, B=10110110 → 4, C=10100010 → 16, D=00110010 → 128.
- Metryka: `d(A,A)=0`, symetria, nierówność trójkąta (`(A⊕B)⊕(B⊕C)=A⊕C` oraz `X⊕Y ≤ X+Y`) (Maymounkov i Mazières).
- k-bucket `B_i = {P : 2^i ≤ d(self,P) < 2^{i+1}}`; lookup ma średnio O(log N) kroków, z równoległymi zapytaniami (α).
- **XOR mierzy odległość logiczną, nie geograficzną.** Węzeł w Japonii może być „bliżej” niż sąsiad. IPFS łagodzi opóźnienia równoległymi zapytaniami, preferowaniem responsywnych peerów i tym, że Bitswap pyta najpierw już połączonych peerów. RTT do Japonii to setki ms, nie sekundy. W M5 nie ma polityki RTT.

## 8. Ograniczenia M5

MemoryStore (po restarcie trzeba ponowić `provide`, brak `announce_all` po enumeracji Blockstore, bo Blockstore nie ma `list`); `find` ≠ pobranie (to robi Bitswap); `stop_providing` nie usuwa rekordów u innych od razu; obecność w k-bucket ≠ otwarte połączenie; konfiguracja Kubo `Routing.Type=custom` jest eksperymentalna.
