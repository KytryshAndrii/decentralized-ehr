
#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# EHR-IPFS: Test Environment Setup
# Uruchom z katalogu głównego repozytorium.
# ============================================================

if [[ ! -f "Cargo.toml" ]]; then
    echo "ERROR: Brak Cargo.toml."
    echo "Uruchom skrypt z katalogu glownego repozytorium."
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "ERROR: Cargo nie jest dostepne."
    exit 1
fi

echo "==> Tworzenie katalogow..."

mkdir -p \
    tests/unit \
    tests/integration \
    tests/interoperability/results \
    tests/fixtures \
    scripts/testing

# Nie zapisujemy danych do fixtures.
for file in hello.txt binary.dat sample.pdf; do
    if [[ ! -e "tests/fixtures/$file" ]]; then
        touch "tests/fixtures/$file"
    fi
done

touch tests/interoperability/results/.gitkeep

# ============================================================
# Osobny crate test-runner
# ============================================================

echo "==> Konfiguracja test-runner..."

if [[ ! -f "tests/test-runner/Cargo.toml" ]]; then
    mkdir -p tests/test-runner

    cargo init \
        --lib \
        --name test-runner \
        --vcs none \
        tests/test-runner
fi

mkdir -p tests/test-runner/tests

if [[ ! -f "tests/test-runner/tests/smoke_test.rs" ]]; then
    cat > tests/test-runner/tests/smoke_test.rs <<'EOF'
#[test]
fn test_environment_works() {
    assert_eq!(2 + 2, 4);
}
EOF
fi

# ============================================================
# Dokumentacja
# ============================================================

if [[ ! -f "tests/unit/README.md" ]]; then
    cat > tests/unit/README.md <<'EOF'
# Unit Tests

Testy jednostkowe implementujemy przede wszystkim
bezpośrednio w poszczególnych bibliotekach Rust.

Przykład:

`crates/ipfs-cid/src/lib.rs`

Uruchomienie:

```bash
cargo test --workspace --lib
```
EOF
fi

if [[ ! -f "tests/integration/README.md" ]]; then
    cat > tests/integration/README.md <<'EOF'
# Integration Tests

Testy integracyjne sprawdzaja wspolprace kilku
komponentow Rust.

Dedykowane testy moga znajdowac sie w:

`tests/test-runner/tests/`

Uruchomienie:

```bash
cargo test -p test-runner
```
EOF
fi

if [[ ! -f "tests/interoperability/README.md" ]]; then
    cat > tests/interoperability/README.md <<'EOF'
# Interoperability Tests

Testy sprawdzaja zgodnosc wlasnej implementacji
IPFS w Rust z referencyjnym wezlem Kubo.

Planowane scenariusze:

- CIDv1 Rust vs Kubo
- libp2p Rust vs Kubo
- Kademlia DHT Rust vs Kubo
- Bitswap Rust vs Kubo
- UnixFS Rust vs Kubo

Wyniki testow zapisujemy w `results/`.
EOF
fi

# ============================================================
# Skrypt: CID test reference
# ============================================================

if [[ ! -f "scripts/testing/test-cid.sh" ]]; then
    cat > scripts/testing/test-cid.sh <<'EOF'
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
EOF
fi

# ============================================================
# Skrypt: sprawdzenie obu wezlow Kubo
# ============================================================

if [[ ! -f "scripts/testing/test-kubo-nodes.sh" ]]; then
    cat > scripts/testing/test-kubo-nodes.sh <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

NODE_A="${NODE_A:-$HOME/ipfs-lab/node-a}"
NODE_B="${NODE_B:-$HOME/ipfs-lab/node-b}"

echo "==> Node A"
IPFS_PATH="$NODE_A" ipfs id -f='<id>\n'

echo "==> Node B"
IPFS_PATH="$NODE_B" ipfs id -f='<id>\n'

PEER_A="$(
    IPFS_PATH="$NODE_A" ipfs id -f='<id>'
)"

echo "==> Connecting B to A..."

IPFS_PATH="$NODE_B" \
    ipfs swarm connect \
    "/ip4/127.0.0.1/tcp/4101/p2p/$PEER_A"

echo "==> Checking connection..."
IPFS_PATH="$NODE_B" ipfs swarm peers

echo "PASS: Kubo nodes connected"
EOF
fi

# ============================================================
# Skrypt: testy Rust
# ============================================================

if [[ ! -f "scripts/testing/run-rust-tests.sh" ]]; then
    cat > scripts/testing/run-rust-tests.sh <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

echo "[1/4] Formatting..."
cargo fmt --all -- --check

echo "[2/4] Compilation..."
cargo check --workspace --all-targets

echo "[3/4] Clippy..."
cargo clippy --workspace --all-targets -- -D warnings

echo "[4/4] Tests..."
cargo test --workspace

echo "PASS: Rust checks"
EOF
fi

chmod +x scripts/testing/*.sh

# ============================================================
# Rejestracja crate w Cargo Workspace
# ============================================================

echo "==> Rejestracja test-runner w workspace..."

# Cargo automatycznie uwzglednia pakiety znajdujace
# sie pod root workspace, jezeli nie sa wykluczone
# i zostaly utworzone odpowiednio w workspace.
#
# Mimo to dla przejrzystosci zalecamy jawny wpis
# w glownym Cargo.toml:
#
# "tests/test-runner"

if cargo metadata --no-deps --format-version 1 \
    | grep -q '"name":"test-runner"'; then
    echo "test-runner rozpoznany przez Cargo."
else
    echo "UWAGA: Dodaj do [workspace].members:"
    echo '    "tests/test-runner",'
fi

echo ""
echo "==> Koniec konfiguracji."
echo "Nastepne polecenia:"
echo "  cargo test -p test-runner"
echo "  ./scripts/testing/test-kubo-nodes.sh"
echo "  ./scripts/testing/test-cid.sh"
echo "  ./scripts/testing/run-rust-tests.sh"

