#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
IP="${M5_TEST_IP:-127.0.0.1}"
KUBO_PORT="${M5_KUBO_PORT:-4201}"
RUST_PORT="${M5_RUST_PORT:-4301}"
PIDS=()
cleanup() {
  for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
  rm -rf "$TMP"
}
trap cleanup EXIT

wait_for() {
  local file="$1" pattern="$2" limit="${3:-60}"
  for ((i=0; i<limit*2; i++)); do
    if grep -Fq -- "$pattern" "$file" 2>/dev/null; then return 0; fi
    sleep 0.5
  done
  echo "TIMEOUT: $pattern in $file" >&2
  tail -n 30 "$file" >&2 || true
  return 1
}

command -v ipfs >/dev/null || { echo 'Kubo ipfs binary is missing' >&2; exit 1; }
(cd "$ROOT" && cargo build -p ehr-cli)
BIN="$ROOT/target/debug/ehr-cli"
mkdir -p "$TMP/kubo" "$TMP/rust1" "$TMP/rust2"
export IPFS_PATH="$TMP/kubo"

ipfs init --profile=test >/dev/null
ipfs bootstrap rm --all >/dev/null
ipfs config --json Addresses.Swarm "[\"/ip4/$IP/tcp/$KUBO_PORT\"]"
ipfs config Addresses.API '/ip4/127.0.0.1/tcp/5201'
ipfs config Addresses.Gateway '/ip4/127.0.0.1/tcp/8201'
ipfs config --json Routing.LoopbackAddressesOnLanDHT true
ipfs config Routing.Type custom
ipfs config --json Routing.Routers '{"LanDHT":{"Type":"dht","Parameters":{"Mode":"server","PublicIPNetwork":false,"AcceleratedDHTClient":false}}}'
ipfs config --json Routing.Methods '{"provide":{"RouterName":"LanDHT"},"find-providers":{"RouterName":"LanDHT"},"find-peers":{"RouterName":"LanDHT"},"put-ipns":{"RouterName":"LanDHT"},"get-ipns":{"RouterName":"LanDHT"}}'
ipfs config --json Provide.DHT.SweepEnabled false
ipfs daemon > "$TMP/kubo.log" 2>&1 &
PIDS+=("$!")
wait_for "$TMP/kubo.log" 'Daemon is ready' 60
KUBO_ID="$(ipfs id -f='<id>')"

start_rust() {
  local name="$1" port="$2"
  mkfifo "$TMP/$name.in"
  (cd "$TMP/$name" && exec "$BIN" node start "/ip4/$IP/tcp/$port" --bootstrap "/ip4/$IP/tcp/$KUBO_PORT/p2p/$KUBO_ID" --dht-server <> "$TMP/$name.in" > "$TMP/$name.log" 2>&1) &
  PIDS+=("$!")
  wait_for "$TMP/$name.log" "CONNECTED $KUBO_ID" 45
  wait_for "$TMP/$name.log" 'DHT_BOOTSTRAP_STARTED' 45
}

printf 'Rust announces a real Blockstore block\n' > "$TMP/rust1/block.txt"
CID_A="$(cd "$TMP/rust1" && "$BIN" put "$TMP/rust1/block.txt" | sed -n 's/.*\[BLOCKSTORE\] PUT CID=//p' | tail -n 1)"
test -n "$CID_A"
start_rust rust1 "$RUST_PORT"
RUST1_ID="$(sed -n 's/^LOCAL_PEER_ID //p' "$TMP/rust1.log" | head -n 1)"
printf 'provide %s\n' "$CID_A" > "$TMP/rust1.in"
wait_for "$TMP/rust1.log" 'DHT_PROVIDING_STARTED' 60

echo 'A: Rust provides, Kubo finds'
KUBO_FOUND="$(timeout 70 ipfs routing findprovs -n 10 "$CID_A")"
grep -Fq "$RUST1_ID" <<< "$KUBO_FOUND" || { echo "Kubo failed to find Rust: $KUBO_FOUND" >&2; exit 1; }

printf 'Kubo announces a block\n' > "$TMP/kubo-block.txt"
CID_B="$(ipfs add -Q --cid-version=1 --raw-leaves "$TMP/kubo-block.txt")"
ipfs routing provide "$CID_B"
echo 'B: Kubo provides, Rust finds'
printf 'find %s\n' "$CID_B" > "$TMP/rust1.in"
wait_for "$TMP/rust1.log" "DHT_PROVIDER $KUBO_ID" 60

start_rust rust2 "$((RUST_PORT+1))"
echo 'C: second Rust node finds first Rust provider through Kubo'
printf 'find %s\n' "$CID_A" > "$TMP/rust2.in"
wait_for "$TMP/rust2.log" "DHT_PROVIDER $RUST1_ID" 60
echo 'PASS: Rust <-> Kubo LAN DHT provider discovery'
