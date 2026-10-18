
#!/usr/bin/env bash
set -euo pipefail

NODE_A="${NODE_A:-$HOME/ipfs-lab/node-a}"
NODE_B="${NODE_B:-$HOME/ipfs-lab/node-b}"

API_A="http://127.0.0.1:5101"
API_B="http://127.0.0.1:5102"

P2P_PORT_A=4101
P2P_PORT_B=4102

PID_A=""
PID_B=""

LOG_DIR="$(mktemp -d)"

# ============================================================
# Cleanup - wykonywany przy zakonczeniu skryptu
# ============================================================

cleanup() {
    local exit_code=$?

    trap - EXIT INT TERM

    echo ""
    echo "==> Stopping test daemons..."

    for pid in "$PID_A" "$PID_B"; do
        if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done

    for pid in "$PID_A" "$PID_B"; do
        if [[ -n "$pid" ]]; then
            wait "$pid" 2>/dev/null || true
        fi
    done

    if [[ "$exit_code" -ne 0 ]]; then
        echo ""
        echo "==> Node A logs:"
        cat "$LOG_DIR/node-a.log"

        echo ""
        echo "==> Node B logs:"
        cat "$LOG_DIR/node-b.log"
    fi

    rm -rf "$LOG_DIR"

    if [[ "$exit_code" -eq 0 ]]; then
        echo "PASS: Test completed successfully."
    else
        echo "FAIL: Test failed (exit code: $exit_code)."
    fi

    exit "$exit_code"
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# ============================================================
# Sprawdzenie konfiguracji
# ============================================================

echo "==> Checking Kubo repositories..."

for node in "$NODE_A" "$NODE_B"; do
    if [[ ! -f "$node/config" ]]; then
        echo "ERROR: Missing Kubo repository: $node"
        exit 1
    fi
done

if ! command -v ipfs >/dev/null 2>&1; then
    echo "ERROR: ipfs executable not found."
    exit 1
fi

PEER_A=$(IPFS_PATH="$NODE_A" ipfs config Identity.PeerID)
PEER_B=$(IPFS_PATH="$NODE_B" ipfs config Identity.PeerID)

echo "Node A: $PEER_A"
echo "Node B: $PEER_B"

if [[ "$PEER_A" == "$PEER_B" ]]; then
    echo "ERROR: Both nodes have the same Peer ID."
    exit 1
fi

# ============================================================
# Sprawdzenie, czy porty API nie sa juz zajete
# ============================================================

echo "==> Checking available ports..."

for port in 4101 4102 5101 5102 8101 8102; do
    if ss -ltnH "( sport = :$port )" | grep -q .; then
        echo "ERROR: Port $port is already in use."
        echo "Stop existing Kubo test daemons before running."
        exit 1
    fi
done

# ============================================================
# Uruchomienie daemonow
# ============================================================

echo "==> Starting Node A..."

IPFS_PATH="$NODE_A" ipfs daemon \
    > "$LOG_DIR/node-a.log" 2>&1 &
PID_A=$!

echo "Node A PID: $PID_A"

echo "==> Starting Node B..."

IPFS_PATH="$NODE_B" ipfs daemon \
    > "$LOG_DIR/node-b.log" 2>&1 &
PID_B=$!

echo "Node B PID: $PID_B"

# ============================================================
# Oczekiwanie na gotowosc RPC
# ============================================================

wait_for_rpc() {
    local node_name="$1"
    local api_url="$2"
    local pid="$3"

    echo "==> Waiting for $node_name RPC..."

    for attempt in {1..30}; do

        if ! kill -0 "$pid" 2>/dev/null; then
            echo "ERROR: $node_name daemon exited."
            return 1
        fi

        if curl --fail --silent \
            --max-time 2 \
            -X POST "$api_url/api/v0/id" \
            > /dev/null; then

            echo "$node_name RPC ready."
            return 0
        fi

        sleep 1
    done

    echo "ERROR: Timeout waiting for $node_name RPC."
    return 1
}

wait_for_rpc "Node A" "$API_A" "$PID_A"
wait_for_rpc "Node B" "$API_B" "$PID_B"

# ============================================================
# Test polaczenia P2P
# ============================================================

echo "==> Connecting Node B to Node A..."

timeout 15s env IPFS_PATH="$NODE_B" \
    ipfs swarm connect \
    "/ip4/127.0.0.1/tcp/$P2P_PORT_A/p2p/$PEER_A"

echo "==> Verifying connection from Node B..."

PEERS_B=$(
    timeout 10s env IPFS_PATH="$NODE_B" \
        ipfs swarm peers
)

if ! grep -Fq "/p2p/$PEER_A" <<< "$PEERS_B"; then
    echo "ERROR: Node B cannot see Node A."
    exit 1
fi

echo "==> Verifying connection from Node A..."

PEERS_A=$(
    timeout 10s env IPFS_PATH="$NODE_A" \
        ipfs swarm peers
)

if ! grep -Fq "/p2p/$PEER_B" <<< "$PEERS_A"; then
    echo "ERROR: Node A cannot see Node B."
    exit 1
fi

echo ""
echo "========================================="
echo " PASS: Kubo P2P connectivity"
echo "========================================="
echo "Node A: $PEER_A"
echo "Node B: $PEER_B"
echo ""
echo "Both nodes are connected."
