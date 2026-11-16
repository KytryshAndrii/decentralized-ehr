#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
IP="${M6_TEST_IP:-127.0.0.1}"
KUBO_PORT="${M6_KUBO_PORT:-4211}"
RUST_PORT="${M6_RUST_PORT:-4311}"
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
mkdir -p "$TMP/kubo" "$TMP/rust"
export IPFS_PATH="$TMP/kubo"

ipfs init --profile=test >/dev/null
ipfs bootstrap rm --all >/dev/null
ipfs config --json Addresses.Swarm "[\"/ip4/$IP/tcp/$KUBO_PORT\"]"
ipfs config Addresses.API '/ip4/127.0.0.1/tcp/5211'
ipfs config Addresses.Gateway '/ip4/127.0.0.1/tcp/8211'
ipfs config --json Routing.LoopbackAddressesOnLanDHT true
ipfs config Routing.Type custom
ipfs config --json Routing.Routers '{"LanDHT":{"Type":"dht","Parameters":{"Mode":"server","PublicIPNetwork":false,"AcceleratedDHTClient":false}}}'
ipfs config --json Routing.Methods '{"provide":{"RouterName":"LanDHT"},"find-providers":{"RouterName":"LanDHT"},"find-peers":{"RouterName":"LanDHT"},"put-ipns":{"RouterName":"LanDHT"},"get-ipns":{"RouterName":"LanDHT"}}'
ipfs config --json Provide.DHT.SweepEnabled false
ipfs daemon > "$TMP/kubo.log" 2>&1 &
PIDS+=("$!")
wait_for "$TMP/kubo.log" 'Daemon is ready' 60
KUBO_ID="$(ipfs id -f='<id>')"

put_block() {
  (cd "$TMP/rust" && "$BIN" put "$1" | sed -n 's/.*\[BLOCKSTORE\] PUT CID=//p' | tail -n 1)
}

printf 'Rust serves this EHR test block over Bitswap\n' > "$TMP/shared.txt"
head -c 1048576 /dev/urandom > "$TMP/large.bin"
printf 'This block is stored but never provided\n' > "$TMP/private.txt"
CID_SHARED="$(put_block "$TMP/shared.txt")"
CID_LARGE="$(put_block "$TMP/large.bin")"
CID_PRIVATE="$(put_block "$TMP/private.txt")"
test -n "$CID_SHARED" && test -n "$CID_LARGE" && test -n "$CID_PRIVATE"
# CID wyliczony bez zapisu: nie ma go żaden węzeł.
printf 'This block exists on no node\n' > "$TMP/nobody.txt"
CID_NOBODY="$("$BIN" cid "$TMP/nobody.txt" | sed -n 's/.* CID=//p')"
test -n "$CID_NOBODY"

mkfifo "$TMP/rust.in"
(cd "$TMP/rust" && EHR_LISTEN_PORT="$RUST_PORT" exec "$BIN" node start --bootstrap "/ip4/$IP/tcp/$KUBO_PORT/p2p/$KUBO_ID" --dht-server <> "$TMP/rust.in" > "$TMP/rust.log" 2>&1) &
PIDS+=("$!")
wait_for "$TMP/rust.log" "BITSWAP_LISTENING /ipfs/bitswap/1.2.0" 45
wait_for "$TMP/rust.log" "CONNECTED $KUBO_ID" 45
RUST_ID="$(sed -n 's/^LOCAL_PEER_ID //p' "$TMP/rust.log" | head -n 1)"

echo 'A: Rust advertises Bitswap 1.2.0 in Identify'
for ((i=0; i<40; i++)); do
  if ipfs id "$RUST_ID" 2>/dev/null | grep -Fq '/ipfs/bitswap/1.2.0'; then break; fi
  sleep 0.5
done
ipfs id "$RUST_ID" | grep -Fq '/ipfs/bitswap/1.2.0' || { echo 'Kubo does not see Bitswap on Rust' >&2; exit 1; }

printf 'provide %s\n' "$CID_SHARED" > "$TMP/rust.in"
printf 'provide %s\n' "$CID_LARGE" > "$TMP/rust.in"
wait_for "$TMP/rust.log" "DHT_PROVIDE_REQUESTED $CID_LARGE" 30

echo 'B: Kubo fetches a small provided block from Rust'
timeout 60 ipfs block get "$CID_SHARED" > "$TMP/shared.out"
cmp "$TMP/shared.txt" "$TMP/shared.out"
wait_for "$TMP/rust.log" "BITSWAP_BLOCK_SENT $KUBO_ID $CID_SHARED" 10

echo 'C: Kubo fetches a 1 MiB provided block from Rust'
timeout 60 ipfs block get "$CID_LARGE" > "$TMP/large.out"
cmp "$TMP/large.bin" "$TMP/large.out"
wait_for "$TMP/rust.log" "BITSWAP_BLOCK_SENT $KUBO_ID $CID_LARGE" 10

echo 'D: a stored but not provided block is not served'
if timeout 15 ipfs block get "$CID_PRIVATE" > "$TMP/private.out" 2>/dev/null; then
  echo 'Kubo fetched a block that Rust never provided' >&2
  exit 1
fi
if grep -Fq "BITSWAP_BLOCK_SENT $KUBO_ID $CID_PRIVATE" "$TMP/rust.log"; then
  echo 'Rust sent a block that was never provided' >&2
  exit 1
fi

# M7: Rust jako klient Bitswap, Kubo jako dostawca.
rust_get() {
  (cd "$TMP/rust" && "$BIN" get "$1" "$2" >/dev/null)
}

echo 'E: Rust finds Kubo in the DHT and fetches a small block over Bitswap'
printf 'Kubo serves this EHR test block over Bitswap\n' > "$TMP/kubo-small.txt"
CID_KUBO_SMALL="$(ipfs block put --cid-codec=raw --mhtype=sha2-256 < "$TMP/kubo-small.txt")"
ipfs routing provide "$CID_KUBO_SMALL"
printf 'fetch %s\n' "$CID_KUBO_SMALL" > "$TMP/rust.in"
wait_for "$TMP/rust.log" "BITSWAP_BLOCK_RECEIVED $KUBO_ID $CID_KUBO_SMALL" 60
rust_get "$CID_KUBO_SMALL" "$TMP/kubo-small.out"
cmp "$TMP/kubo-small.txt" "$TMP/kubo-small.out"

echo 'F: Rust fetches a 1 MiB block from Kubo'
head -c 1048576 /dev/urandom > "$TMP/kubo-large.bin"
CID_KUBO_LARGE="$(ipfs block put --cid-codec=raw --mhtype=sha2-256 < "$TMP/kubo-large.bin")"
ipfs routing provide "$CID_KUBO_LARGE"
printf 'fetch %s\n' "$CID_KUBO_LARGE" > "$TMP/rust.in"
wait_for "$TMP/rust.log" "BITSWAP_BLOCK_RECEIVED $KUBO_ID $CID_KUBO_LARGE" 60
rust_get "$CID_KUBO_LARGE" "$TMP/kubo-large.out"
cmp "$TMP/kubo-large.bin" "$TMP/kubo-large.out"

echo 'G: a block nobody provides is not fetched'
printf 'fetch %s\n' "$CID_NOBODY" > "$TMP/rust.in"
wait_for "$TMP/rust.log" "BITSWAP_FETCH_FAILED $CID_NOBODY" 60
if rust_get "$CID_NOBODY" "$TMP/missing.out" 2>/dev/null; then
  echo 'Rust stored a block that nobody provided' >&2
  exit 1
fi

grep -F 'BITSWAP_' "$TMP/rust.log" | tail -n 30
echo 'PASS: Kubo and Rust fetched blocks from each other over Bitswap 1.2.0'
