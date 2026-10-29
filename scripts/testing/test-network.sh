#!/usr/bin/env bash
set -euo pipefail

# Rust <-> Kubo interoperability. This test ALWAYS shuts down the Kubo daemon
# associated with IPFS_PATH on exit, including an already running daemon.
# Use a dedicated test repo, never your everyday ~/.ipfs.
CLI="${EHR_CLI_BIN:-target/debug/ehr-cli}"

# Default dedicated Kubo test repository.
# Can be overridden using IPFS_PATH.
export IPFS_PATH="${IPFS_PATH:-$HOME/ipfs-lab/node-a}"
command -v ipfs >/dev/null || { echo 'ERROR: ipfs missing' >&2; exit 1; }
[[ -f "$IPFS_PATH/config" ]] || { echo "ERROR: missing repo: $IPFS_PATH" >&2; exit 1; }
[[ -x "$CLI" ]] || { echo 'ERROR: cargo build -p ehr-cli first' >&2; exit 1; }

TMP=$(mktemp -d)
NODE_PID=''
KUBO_PID=''
KUBO_USED=false

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  if [[ -n "$NODE_PID" ]]; then
    kill "$NODE_PID" 2>/dev/null || true
    wait "$NODE_PID" 2>/dev/null || true
  fi
  if [[ "$KUBO_USED" == true ]]; then
    echo 'Stopping Kubo for IPFS_PATH (also if already running before the test)...'
    IPFS_PATH="$IPFS_PATH" timeout 10s ipfs shutdown >/dev/null 2>&1 || true
  fi
  if [[ -n "$KUBO_PID" ]]; then
    # The PID belongs to the daemon launched directly by this script.
    for _ in $(seq 1 20); do
      kill -0 "$KUBO_PID" 2>/dev/null || break
      sleep 0.2
    done
    if kill -0 "$KUBO_PID" 2>/dev/null; then
      kill "$KUBO_PID" 2>/dev/null || true
    fi
    wait "$KUBO_PID" 2>/dev/null || true
  fi
  if (( status != 0 )); then
    echo '=== Kubo log ===' >&2
    [[ ! -f "$TMP/kubo.log" ]] || tail -n 60 "$TMP/kubo.log" >&2
    echo '=== Rust log ===' >&2
    [[ ! -f "$TMP/rust.log" ]] || tail -n 60 "$TMP/rust.log" >&2
  fi
  rm -rf "$TMP"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

if IPFS_PATH="$IPFS_PATH" timeout 4s ipfs swarm addrs listen >/dev/null 2>&1; then
  echo 'Kubo already running; test will shut it down at exit.'
  KUBO_USED=true
else
  echo 'Starting Kubo...'
  IPFS_PATH="$IPFS_PATH" ipfs daemon >"$TMP/kubo.log" 2>&1 &
  KUBO_PID=$!
  KUBO_USED=true
  ready=false
  for _ in $(seq 1 60); do
    if IPFS_PATH="$IPFS_PATH" timeout 3s ipfs swarm addrs listen >/dev/null 2>&1; then
      ready=true
      break
    fi
    kill -0 "$KUBO_PID" 2>/dev/null || { echo 'Kubo exited during startup' >&2; exit 1; }
    sleep 0.5
  done
  [[ "$ready" == true ]] || { echo 'Timeout starting Kubo' >&2; exit 1; }
fi

KUBO_ID=$(IPFS_PATH="$IPFS_PATH" ipfs id -f='<id>')
KUBO_ADDR=$(IPFS_PATH="$IPFS_PATH" ipfs swarm addrs listen | grep -E '^/ip4/127\.0\.0\.1/tcp/[0-9]+$' | head -n 1 || true)
[[ -n "$KUBO_ADDR" ]] || { echo 'Kubo needs a 127.0.0.1 TCP swarm listener' >&2; exit 1; }
echo "Kubo: ${KUBO_ADDR}/p2p/${KUBO_ID}"

"$CLI" node start "$TMP/rust.key.pb" /ip4/127.0.0.1/tcp/0 "${KUBO_ADDR}/p2p/${KUBO_ID}" >"$TMP/rust.log" 2>&1 &
NODE_PID=$!
RUST_ADDR=''
for _ in $(seq 1 40); do
  RUST_ADDR=$(sed -n 's/^LISTEN //p' "$TMP/rust.log" | head -n 1)
  [[ -z "$RUST_ADDR" ]] || break
  kill -0 "$NODE_PID" 2>/dev/null || { echo 'Rust node exited' >&2; exit 1; }
  sleep 0.5
done
[[ -n "$RUST_ADDR" ]] || { echo 'Rust listen timeout' >&2; exit 1; }
echo "Rust: $RUST_ADDR"

IPFS_PATH="$IPFS_PATH" timeout 15s ipfs swarm connect "$RUST_ADDR"
identified=false
for _ in $(seq 1 40); do
  if grep -q '^IDENTIFY ' "$TMP/rust.log"; then identified=true; break; fi
  sleep 0.5
done
[[ "$identified" == true ]] || { echo 'No Identify event' >&2; exit 1; }
grep -q '^CONNECTED ' "$TMP/rust.log"
RUST_ID="${RUST_ADDR##*/p2p/}"
IPFS_PATH="$IPFS_PATH" ipfs swarm peers | grep -Fq "$RUST_ID"
IPFS_PATH="$IPFS_PATH" timeout 15s ipfs id "$RUST_ID" >/dev/null
IPFS_PATH="$IPFS_PATH" timeout 20s ipfs ping -n 2 "$RUST_ID"
echo 'PASS: Rust <-> Kubo TCP/Noise/Yamux/Identify/Ping'
