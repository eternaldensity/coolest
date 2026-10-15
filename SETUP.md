# Setup & Usage

## Prerequisites

- Rust stable (tested 1.98.1): https://rustup.rs
- C linker — one of:
  - WSL/Debian: `sudo apt update && sudo apt install -y build-essential pkg-config`
  - Windows native: VS Build Tools with “Desktop C++” workload
  - macOS: `xcode-select --install`
- Optional: `rustup component add rustfmt clippy`

> If your shell inherits a Windows `sccache` into WSL (fails with
> `Permission denied`), neutralize it per-command:
> `export SCCACHE_DISABLE=1 RUSTC_WRAPPER=""`

## Build, test, run

```bash
git clone <repo> coolest && cd coolest

# All tests (194 and growing)
cargo test --workspace

# Lint gate (CI blocks on warnings)
cargo clippy --workspace --all-targets -- -D warnings

# Whole merge gate in one shot (fmt, clippy, tests, lexicons, hygiene)
./gate.sh

# Nightly heavy gates (1000-seed gen, 1M-tick soak, power chaos)
./nightly.sh

# Headless smoke demo: survival → deck → net → ledger → crime → veil
cargo run -p client-web
```

Expected demo tail:

```text
survival: food  99  mood  77  $2.30 rations=2
deck: CPU 0/100 (0%) killed=true
warden: guard=true traced=true foothold=true
net: 1→3 45ms toll $0.10 cap=10 gateway_ok=true trace_thr=8
ledger: a=700 b=300 audit_ok=true
mail: inbox=1
crime: align=Untrusted standing=Fined
license eligible (dirty record) = false
veil: present after death = false
funnel: funnel started=0 job=0 hack=21 traced=21 survived=-
demo ok, log lines = 2
```

## Web build (trunk 0.22+, wasm32 target installed)

```bash
cd gui && trunk serve   # serves the eframe shell; Desktop shell: cargo run -p gui
```

Notes: workspace-root `trunk build` fails on trunk's package discovery —
always run from `gui/`. Network/tokio code is compiled out on wasm
(see `server/Cargo.toml` target gating).

## Layout

```text
sim/            # deterministic game sim (no clock/net/fs/threads)
  src/{world,scheduler,ids,pos,renderer,street,civic,
       deck,net,ledger,crime,veil}.rs
client-web/     # headless web-client core + demo binary
  src/{lib,main}.rs
data/tuning.yaml
design/         # 00–12 full design docs
```

`sim` is UI-agnostic: it emits serializable `RenderView`s (Street/Net/Deck);
any client (web/Bevy/Godot) implements `sim::Renderer`. See
`design/08-architecture-tech.md` and `design/11-architecture-standards.md`.

## Workflow (enforced)

- One commit per unit of work (`git log --oneline` shows the pattern).
- New/changed behavior ships with a test; balance numbers ship with gates
  (`design/12-testing-protocols.md`).
- `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings`
  green before every commit.
