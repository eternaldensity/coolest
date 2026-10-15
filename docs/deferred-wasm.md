# wasm publishing via trunk — resolved

> Status: **resolved 2026-10-06**. Kept for the diagnosis, which is the
> useful part if the pin ever regresses.

## Working setup

- `cargo install trunk --version "=0.22.0-rc.1"` compiles clean on
  rustc 1.98 (the 0.21/lightningcss skew is gone in 0.22).
- Run trunk from the **member** directory, not the workspace root:
  `cd gui && trunk build` / `trunk serve`. A root-level run fails with
  `could not find the root package of the target crate` because the
  workspace root is virtual.
- Canonical template is `gui/index.html`; `gui/dist/` is gitignored build
  output.
- Verified: a ~30MB dev bundle (wasm + JS glue + index.html) builds green.

Still open (not blocking): release + `wasm-opt` shrinking.

## The original problem (2026-10-06)

`cargo install trunk` (0.21.14, latest at the time) failed on rustc 1.98.1:
its pinned `lightningcss` died with 61 errors (E0053/E0277/E0308). Our code
was not involved — any crate would have hit it.

## If it regresses

1. `cargo search trunk` first: a newer release may have re-pinned.
2. Pin a period compiler for the older dependency era:
   `rustup toolchain install 1.85 && cargo +1.85 install trunk --locked`.
3. Skip bundlers entirely: `cargo build --target wasm32-unknown-unknown`
   plus a hand-rolled `wasm-bindgen` loader.
