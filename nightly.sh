#!/usr/bin/env bash
# Nightly heavy gates (design/12 section 1): 1000-seed gen, 1M-tick soak,
# chaos with power drops. PDS kill is unavailable: no PDS bridge yet, so
# chaos covers power kill/restore + blackout routing only.
set -euo pipefail
cd "$(dirname "$0")"
export SCCACHE_DISABLE=1 RUSTC_WRAPPER=""

echo "== gen 1000 seeds =="
cargo run -q -p tools -- gen 1000 0

echo "== mapgen 200 seeds =="
cargo run -q -p tools -- mapgen-check 200 0 | tail -1

echo "== soak 1M ticks =="
cargo run -q -p tools -- soak 100 1000000 7

echo "== chaos 100k ticks =="
cargo run -q -p tools -- chaos 100 100000 7

echo "NIGHTLY GREEN (PDS chaos unavailable: no PDS bridge yet)"
