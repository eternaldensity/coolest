# Deferred: wasm publishing via trunk

## Problem (2026-10-06)

`cargo install trunk` (0.21.14, latest at the time) fails on `rustc 1.98.1`:
its pinned `lightningcss` dies with 61 errors (E0053/E0277/E0308).
Our code is not involved — any crate would hit this.

## Retry order (when wasm publishing actually matters)

1. `rustup toolchain install 1.85 && cargo +1.85 install trunk --locked`
   (period compiler for trunk 0.21's dependency era — highest odds)
2. `cargo search trunk` first — a 0.22+ may have fixed the pin by then
3. Skip bundlers entirely: `cargo build --target wasm32-unknown-unknown`
   + hand-rolled `wasm-bindgen` loader

## Why deferred

The desktop eframe shell is the current proving ground and needs no
bundler. Do not spend the ~15 min trunk compile until a wasm build is
on the critical path.
