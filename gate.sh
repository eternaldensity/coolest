#!/usr/bin/env bash
# Local merge gate (design/12 section 1, minus proptest/nightly/human stages).
# Fails fast. Needs only rustfmt + python3 stdlib; no network, no extra installs.
set -euo pipefail
cd "$(dirname "$0")"
export SCCACHE_DISABLE=1 RUSTC_WRAPPER=""

echo "== fmt =="
cargo fmt --all -- --check

echo "== clippy =="
cargo clippy --workspace --all-targets -- -D warnings

echo "== tests (incl. goldens + data-file parsing) =="
cargo test --workspace

echo "== lexicons parse =="
python3 -c "import json,glob; [json.load(open(p)) for p in glob.glob('lexicons/*.json')]; print('lexicons ok')"

echo "== yaml/json hygiene (no tabs, no trailing whitespace) =="
if grep -rnP '\t| $' data/*.yaml data/*.json lexicons/*.json 2>/dev/null; then
  echo "hygiene FAILED"; exit 1
fi
echo "hygiene ok"

echo "GATE GREEN"
