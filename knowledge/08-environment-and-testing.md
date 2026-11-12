# 08 — Środowisko, maszyny, Kubo i polecenia testowe

## 1. Maszyny (sieć domowa 192.168.1.0/24, router 192.168.1.1)

| Host | System | Adres | Rola w testach |
|---|---|---|---|
| Host A / „Home Server” | Fedora Server (fizyczny), `wlo1` | `192.168.1.125` (DHCP; docker0 172.17.0.1) | Domyślny bootstrap Rust `:4101`, PeerId `12D3KooWNX5AdkfjZ6nSWD8x7Cci1nuNtpXnj1AL42BXi7M9GENS` (w `consts.rs`); firewalld zone `FedoraServer`; Kubo jako usługa `systemctl --user {start,stop,restart,status} kubo`, logi `journalctl --user -u kubo -f` |
| Host B | Ubuntu 24.04 w WSL2 na Windows | WSL2 `172.25.151.102/20` (zmienia się), Windows `192.168.1.51` | Repo `~/ipfs_prototype`; port `4102`; przychodzące wymaga `netsh portproxy` |
| MacBook (lab Kubo) | macOS | localhost | Węzły Kubo w `~/ipfs-lab/node-a` i `node-b` |
| Docelowo | 3 komputery | — | A = placówka, B, C = pacjenci |

Lab Kubo na MacBooku (notatki autora):
- node-a: Swarm `/ip4/127.0.0.1/tcp/4101`, API `5101`, Gateway `8101`
- node-b: Swarm `4102`, API `5102`, Gateway `8102`
- PeerId Kubo: `12D3KooWK2qLa8sF8isj1BUQMMWY6XwbnT8NtzWEHJP3QN38twB3`, `12D3KooWAdKvFoMbRxR57U5zRRTUf6AEjzhRXPHGBG2VT6fG7Jrj`, `12D3KooWRcqmTyYbyASisDqtaoKe5LF8u3weqyN5V12cPL43DYmT`
- ⚠️ Porty 4101/4102 kolidują z domyślnymi portami węzłów Rust na tej samej maszynie, więc dla testów Rust na Macu użyj innych portów.
- Lekcja: `PEER_ID_A` w multiaddr to placeholder, a Kubo zwraca `invalid cid: selected encoding not supported`.

## 2. Wymagania

Rust stable (edition 2024), Kubo w `PATH` (README: v0.43.1+; flagi CLI Kubo zależą od wersji, więc sprawdź `ipfs version`), `ipfs init` dla interop CID.

## 3. Polecenia CLI (stan M5)

```bash
cargo build -p ehr-cli                                   # binarka: target/debug/ehr-cli
./target/debug/ehr-cli node identity                     # LOCAL_PEER_ID (./tmp/node-key.pb)
ehr-cli node start                                       # 127.0.0.1:0 + domyślny bootstrap + Client
ehr-cli node start /ip4/192.168.1.125/tcp/4101 "" --dht-server     # pierwszy Server, bez bootstrapu
ehr-cli node start /ip4/0.0.0.0/tcp/4102 --bootstrap /ip4/192.168.1.125/tcp/4101/p2p/<PEER_A>
ehr-cli node start <listen> --default-bootstrap [--dht-server]
ehr-cli cid|put <ABS_PATH>   ;  ehr-cli has <CID>  ;  ehr-cli get <CID> <ABS_OUT>
# w działającym node start (stdin):
peers | bootstrap | provide <CID> | find <CID> | stop <CID> | quit
```

- Tożsamość i baza zależą od **CWD** (`./tmp/node-key.pb`, `data/ehr-blockstore.leveldb`), więc kilka procesów na jednej maszynie wymaga osobnych katalogów roboczych.
- LevelDB nie może być otwarty przez 2 procesy naraz: `put` i `has` w drugim terminalu przy działającym `node start` mogą kolidować, jeśli węzeł trzyma bazę (obecnie `provide` otwiera bazę tylko na chwilę).

## 4. Testy

```bash
./scripts/testing/run-rust-tests.sh                       # fmt, check, clippy -D warnings, test
cargo test -p ipfs-cid | -p ipfs-store | -p ipfs-network
cargo build -p ehr-cli && cargo test -p test-runner --test integration-network -- --test-threads=1 --nocapture
cargo test -p test-runner --test interop-cid -- --test-threads=1
cargo test -p test-runner --test interop-network -- --nocapture
cargo test -p test-runner --test interop-dht -- --ignored --nocapture --test-threads=1
bash scripts/testing/test-kubo-dht-interop.sh             # M5_TEST_IP, M5_KUBO_PORT=4201, M5_RUST_PORT=4301
```

Interop CID: `ipfs block put --cid-codec=<codec> --mhtype=sha2-256 --offline <file>`, porównanie 1:1 z Rust.
Kubo do testów LAN DHT (z `test-kubo-dht-interop.sh`): `ipfs init --profile=test`, `bootstrap rm --all`, `Routing.LoopbackAddressesOnLanDHT=true`, `Routing.Type=custom` z routerem `LanDHT` (mode server, `PublicIPNetwork:false`), `Provide.DHT.SweepEnabled=false`.

## 5. Firewall i NAT

```bash
sudo firewall-cmd --zone=FedoraServer --add-rich-rule='rule family="ipv4" source address="192.168.1.0/24" port port="4101" protocol="tcp" accept'
nc -vz -w 3 192.168.1.125 4101
```
Windows → WSL2 (PowerShell admin): `netsh interface portproxy add v4tov4 listenaddress=192.168.1.51 listenport=4102 connectaddress=<WSL_IP> connectport=4102` + `New-NetFirewallRule ... -LocalPort 4102`. Po restarcie WSL2 IP się zmienia, więc trzeba zaktualizować portproxy.

## 6. Interpretacja logów

- `DIAL_FAILED ... refused/timed out` przed `CONNECTED` jest normalne (drugi host nie słucha).
- `PeerCondition::NotDialing` oznacza nadmiarowy dial, a nie awarię.
- `DHT_PROVIDER <PeerId>` to tylko znaleziony dostawca, nie pobrany blok (`has` dalej da `false`, dopóki nie ma Bitswap).
- Status testu `ignored` lub `SKIP` nie potwierdza interop. Raportuj go uczciwie.
- Przed oddaniem rozdziału zapisz `git rev-parse HEAD`, wynik `cargo test` i wersje narzędzi.
