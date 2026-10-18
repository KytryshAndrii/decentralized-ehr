#!/usr/bin/env bash
set -euo pipefail

NODE_A="${NODE_A:-$HOME/ipfs-lab/node-a}"

EXPECTED="bafkreibm6jg3ux5qumhcn2b3flc3tyu6dmlb4xa7u5bf44yegnrjhc4yeq"

if ! command -v ipfs >/dev/null 2>&1; then
    echo "FAIL: ipfs CLI not found."
    exit 1
fi

TEST_FILE="$(mktemp)"
trap 'rm -f "$TEST_FILE"' EXIT

printf "hello" > "$TEST_FILE"

CID=$(
    IPFS_PATH="$NODE_A" ipfs block put \
        --cid-codec=raw \
        --mhtype=sha2-256 \
        "$TEST_FILE"
)

echo "Expected: $EXPECTED"
echo "Actual:   $CID"

if [[ "$CID" == "$EXPECTED" ]]; then
    echo "PASS: Kubo CID reference"
else
    echo "FAIL: CID mismatch"
    exit 1
fi
