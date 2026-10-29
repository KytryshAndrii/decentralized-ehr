#!/usr/bin/env bash
set -euo pipefail

NODE_A="${NODE_A:-$HOME/ipfs-lab/node-a}"
NODE_B="${NODE_B:-$HOME/ipfs-lab/node-b}"
API_A='http://127.0.0.1:5101'
API_B='http://127.0.0.1:5102'
PID_A=''
PID_B=''
USED_A=false
USED_B=false
LOG_DIR=$(mktemp -d)

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  echo '==> Stopping Kubo test nodes...'
  # Shut down the *specific* repository's daemon even if it existed beforehand.
  if [[ "$USED_B" == true ]]; then IPFS_PATH="$NODE_B" timeout 10s ipfs shutdown >/dev/null 2>&1 || true; fi
  if [[ "$USED_A" == true ]]; then IPFS_PATH="$NODE_A" timeout 10s ipfs shutdown >/dev/null 2>&1 || true; fi
  for pid in "$PID_A" "$PID_B"; do
    [[ -n "$pid" ]] || continue
    for _ in $(seq 1 20); do kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
    if kill -0 "$pid" 2>/dev/null; then kill "$pid" 2>/dev/null || true; fi
    wait "$pid" 2>/dev/null || true
  done
  if (( status != 0 )); then
    echo '=== Node A log ===' >&2; [[ ! -f "$LOG_DIR/node-a.log" ]] || cat "$LOG_DIR/node-a.log" >&2
    echo '=== Node B log ===' >&2; [[ ! -f "$LOG_DIR/node-b.log" ]] || cat "$LOG_DIR/node-b.log" >&2
  fi
  rm -rf "$LOG_DIR"
  if (( status == 0 )); then echo 'PASS: Kubo P2P connectivity'; else echo "FAIL: exit $status" >&2; fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
command -v ipfs >/dev/null || { echo 'ERROR: ipfs not installed' >&2; exit 1; }
for node in "$NODE_A" "$NODE_B"; do
  [[ -f "$node/config" ]] || { echo "ERROR: missing repository $node" >&2; exit 1; }
done
PEER_A=$(IPFS_PATH="$NODE_A" ipfs config Identity.PeerID)
PEER_B=$(IPFS_PATH="$NODE_B" ipfs config Identity.PeerID)
[[ "$PEER_A" != "$PEER_B" ]] || { echo 'ERROR: duplicate PeerId' >&2; exit 1; }
# The test expects fresh instances using fixed ports.
for port in 4101 4102 5101 5102 8101 8102; do
  if ss -ltnH "( sport = :$port )" | grep -q .; then
    echo "ERROR: test port $port already occupied; use dedicated test nodes" >&2
    exit 1
  fi
done
IPFS_PATH="$NODE_A" ipfs daemon >"$LOG_DIR/node-a.log" 2>&1 & PID_A=$!; USED_A=true
IPFS_PATH="$NODE_B" ipfs daemon >"$LOG_DIR/node-b.log" 2>&1 & PID_B=$!; USED_B=true
wait_for_rpc() {
  local name="$1" url="$2" pid="$3"
  for _ in $(seq 1 30); do
    if curl --fail --silent --max-time 2 -X POST "$url/api/v0/id" >/dev/null; then
      echo "$name RPC ready"
      return 0
    fi
    kill -0 "$pid" 2>/dev/null || { echo "ERROR: $name exited" >&2; return 1; }
    sleep 1
  done
  echo "ERROR: timeout waiting for $name" >&2
  return 1
}
wait_for_rpc 'Node A' "$API_A" "$PID_A"
wait_for_rpc 'Node B' "$API_B" "$PID_B"
timeout 15s env IPFS_PATH="$NODE_B" ipfs swarm connect "/ip4/127.0.0.1/tcp/4101/p2p/$PEER_A"
PEERS_B=$(timeout 10s env IPFS_PATH="$NODE_B" ipfs swarm peers)
grep -Fq "/p2p/$PEER_A" <<<"$PEERS_B" || { echo 'Node B cannot see A' >&2; exit 1; }
PEERS_A=$(timeout 10s env IPFS_PATH="$NODE_A" ipfs swarm peers)
grep -Fq "/p2p/$PEER_B" <<<"$PEERS_A" || { echo 'Node A cannot see B' >&2; exit 1; }
echo "Connected: $PEER_A <-> $PEER_B"
