#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
IP="${M8_TEST_IP:-127.0.0.1}"
KUBO_PORT="${M8_KUBO_PORT:-4212}"
RUST_PORT="${M8_RUST_PORT:-4312}"
PIDS=()
cleanup() {
  for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
  if [[ -n "${M8_LOG_DIR:-}" ]]; then cp "$TMP"/*.log "$M8_LOG_DIR"/ 2>/dev/null || true; fi
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
  grep -a 'FETCH\|FAILED\|REJECTED' "$file" | tail -n 8 >&2 || true
  grep -ac 'BLOCK_RECEIVED' "$file" >&2 || true
  return 1
}

command -v ipfs >/dev/null || { echo 'Kubo ipfs binary is missing' >&2; exit 1; }
(cd "$ROOT" && cargo build -p ehr-cli)
BIN="$ROOT/target/debug/ehr-cli-patient"
mkdir -p "$TMP/kubo" "$TMP/rust"
export IPFS_PATH="$TMP/kubo"

ipfs init --profile=test >/dev/null
ipfs bootstrap rm --all >/dev/null
ipfs config --json Addresses.Swarm "[\"/ip4/$IP/tcp/$KUBO_PORT\"]"
ipfs config Addresses.API '/ip4/127.0.0.1/tcp/5212'
ipfs config Addresses.Gateway '/ip4/127.0.0.1/tcp/8212'
ipfs config --json Routing.LoopbackAddressesOnLanDHT true
ipfs config Routing.Type custom
ipfs config --json Routing.Routers '{"LanDHT":{"Type":"dht","Parameters":{"Mode":"server","PublicIPNetwork":false,"AcceleratedDHTClient":false}}}'
ipfs config --json Routing.Methods '{"provide":{"RouterName":"LanDHT"},"find-providers":{"RouterName":"LanDHT"},"find-peers":{"RouterName":"LanDHT"},"put-ipns":{"RouterName":"LanDHT"},"get-ipns":{"RouterName":"LanDHT"}}'
ipfs config --json Provide.DHT.SweepEnabled false
ipfs daemon > "$TMP/kubo.log" 2>&1 &
PIDS+=("$!")
wait_for "$TMP/kubo.log" 'Daemon is ready' 60
KUBO_ID="$(ipfs id -f='<id>')"

KUBO_ADD=(ipfs add -Q --cid-version=1 --raw-leaves)
rust_add() {
  (cd "$TMP/rust" && "$BIN" add "$1" 2>/dev/null | sed -n 's/.*\[UNIXFS\] ADDED CID=\([a-z0-9]*\) .*/\1/p')
}
rust_get() {
  (cd "$TMP/rust" && "$BIN" get "$1" "$2" >/dev/null 2>&1)
}

# Text and "video" (fixed 256 KiB chunks), a small file (one raw block) and an empty file.
for i in $(seq 1 40000); do
  printf 'Patient %d: observation %d, value %d mmHg\n' $((i % 97)) "$i" $((i * 37 % 200))
done > "$TMP/record.txt"
head -c "${M8_VIDEO_BYTES:-1500000}" /dev/urandom > "$TMP/ekg.mp4"
printf 'small EHR note\n' > "$TMP/note.txt"
: > "$TMP/empty.txt"

echo 'A: Rust and Kubo compute the same CID for the same file and chunker'
declare -A RUST_CID
for name in record.txt ekg.mp4 note.txt empty.txt; do
  chunker='size-262144'
  RUST_CID[$name]="$(rust_add "$TMP/$name")"
  kubo_cid="$("${KUBO_ADD[@]}" -n --chunker="$chunker" "$TMP/$name")"
  test -n "${RUST_CID[$name]}"
  if [[ "${RUST_CID[$name]}" != "$kubo_cid" ]]; then
    echo "CID mismatch for $name: rust=${RUST_CID[$name]} kubo=$kubo_cid" >&2
    exit 1
  fi
  echo "  $name $chunker ${RUST_CID[$name]}"
done

mkfifo "$TMP/rust.in"
(cd "$TMP/rust" && EHR_LISTEN_PORT="$RUST_PORT" exec "$BIN" node start --bootstrap "/ip4/$IP/tcp/$KUBO_PORT/p2p/$KUBO_ID" --dht-server <> "$TMP/rust.in" > "$TMP/rust.log" 2>&1) &
PIDS+=("$!")
wait_for "$TMP/rust.log" "BITSWAP_LISTENING /ipfs/bitswap/1.2.0" 45
wait_for "$TMP/rust.log" "CONNECTED $KUBO_ID" 45

echo 'B: Kubo reads multi-block files imported and provided by Rust'
for name in record.txt ekg.mp4; do
  printf 'provide %s\n' "${RUST_CID[$name]}" > "$TMP/rust.in"
  wait_for "$TMP/rust.log" "DHT_PROVIDE_REQUESTED ${RUST_CID[$name]}" 30
  timeout 120 ipfs cat "${RUST_CID[$name]}" > "$TMP/$name.kubo"
  cmp "$TMP/$name" "$TMP/$name.kubo"
done

# Rust fetches the whole DAG from Kubo and rebuilds the file.
rust_fetches() {
  local label="$1" cid="$2" file="$3"
  ipfs routing provide "$cid"
  printf 'fetch %s\n' "$cid" > "$TMP/rust.in"
  wait_for "$TMP/rust.log" "BITSWAP_FETCH_DONE $cid" 120
  rust_get "$cid" "$TMP/$label.out"
  cmp "$file" "$TMP/$label.out"
}

# Rust no longer builds Rabin DAGs, but reading does not depend on the chunker.
echo 'C: Rust fetches and rebuilds a Kubo file made with the rabin chunker'
head -c 1300000 /dev/urandom > "$TMP/kubo-rabin.bin"
rust_fetches kubo-rabin "$("${KUBO_ADD[@]}" --chunker=rabin "$TMP/kubo-rabin.bin")" "$TMP/kubo-rabin.bin"

echo 'D: Rust rebuilds a two-level balanced Kubo DAG (200 leaves)'
head -c 204800 /dev/urandom > "$TMP/kubo-deep.bin"
rust_fetches kubo-deep "$("${KUBO_ADD[@]}" --chunker=size-1024 "$TMP/kubo-deep.bin")" "$TMP/kubo-deep.bin"

echo 'E: Rust rebuilds a Kubo CIDv0 file (dag-pb leaves, CIDv0 links)'
head -c 700000 /dev/urandom > "$TMP/kubo-v0.bin"
CID_V0="$(ipfs add -Q --cid-version=0 "$TMP/kubo-v0.bin")"
rust_fetches kubo-v0 "$(ipfs cid base32 "$CID_V0")" "$TMP/kubo-v0.bin"

echo 'F: Rust rebuilds a Kubo trickle DAG'
head -c 300000 /dev/urandom > "$TMP/kubo-trickle.bin"
rust_fetches kubo-trickle "$("${KUBO_ADD[@]}" --trickle --chunker=size-1024 "$TMP/kubo-trickle.bin")" "$TMP/kubo-trickle.bin"

echo 'G: a UnixFS directory is rejected by get'
mkdir "$TMP/dir" && cp "$TMP/note.txt" "$TMP/dir/"
CID_DIR="$("${KUBO_ADD[@]}" -r "$TMP/dir")"
ipfs routing provide "$CID_DIR"
printf 'fetch %s\n' "$CID_DIR" > "$TMP/rust.in"
wait_for "$TMP/rust.log" "BITSWAP_FETCH_DONE $CID_DIR" 60
if rust_get "$CID_DIR" "$TMP/dir.out"; then
  echo 'Rust exported a directory as a file' >&2
  exit 1
fi
test ! -e "$TMP/dir.out"

grep -F 'BITSWAP_FETCH' "$TMP/rust.log" | tail -n 12
echo 'PASS: Rust <-> Kubo UnixFS files (same CIDs, exchange in both directions)'
