#!/usr/bin/env bash
set -euo pipefail
NODE_A="${NODE_A:-$HOME/ipfs-lab/node-a}"
EXPECTED='bafkreibm6jg3ux5qumhcn2b3flc3tyu6dmlb4xa7u5bf44yegnrjhc4yeq'
command -v ipfs >/dev/null || { echo 'FAIL: ipfs CLI not found' >&2; exit 1; }
[[ -f "$NODE_A/config" ]] || { echo "FAIL: missing Kubo repository $NODE_A" >&2; exit 1; }
TEST_FILE=$(mktemp)
KUBO_WAS_ONLINE=false
if IPFS_PATH="$NODE_A" timeout 4s ipfs swarm addrs listen >/dev/null 2>&1; then KUBO_WAS_ONLINE=true; fi
cleanup() {
  local status=$?
  trap - EXIT INT TERM
  rm -f "$TEST_FILE"
  # The CID test normally runs offline; shut down a daemon if this repo was online.
  if [[ "$KUBO_WAS_ONLINE" == true ]]; then
    echo 'Stopping Kubo used by CID test...'
    IPFS_PATH="$NODE_A" timeout 10s ipfs shutdown >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
printf 'hello' >"$TEST_FILE"
CID=$(IPFS_PATH="$NODE_A" ipfs block put --cid-codec=raw --mhtype=sha2-256 "$TEST_FILE")
echo "Expected: $EXPECTED"
echo "Actual:   $CID"
[[ "$CID" == "$EXPECTED" ]] || { echo 'FAIL: CID mismatch' >&2; exit 1; }
echo 'PASS: Kubo CID reference'
