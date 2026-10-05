# 11 — Architecture Standards (freeze before M0)

## 1. Hard rules (CI fails if violated)

1. **Server authoritative, deterministic sim.** All gameplay mutation in `sim/` only.
   `sim/` imports: std + `serde` + RNG only. No wall-clock, no threads, no net,
   no filesystem, no `eval`. Tick input = `Intent`, output = `Delta + events`.
2. **ECS, no inheritance.** Entities = `u64`. Components = plain `Serialize` structs.
   Logic = systems. No `PhysicalObject > Item` trees. Cross-refs by ID only, never pointers.
3. **IDs computable.** `TileId = city+sx,sy,z,x,y` math. `HostId`, `EntityId = shard|kind|counter`.
   String forms (`S(-4,32)`) debug-only. No string parse on hot path.
4. **Data over code.** Jobs, timetables, vulns, cards, districts, filters in
   `data/*.yaml` with JSON-schema validation. New content = PR to YAML + test, not Rust edit.
   Mods vetted; no runtime code load.
5. **Ledger closed.** Money/records/licenses via `LedgerService` only.
   Invariant enforced in code: `Σ balances == Σ mints − Σ burns`. Every transfer has source.
   Replicas are views; quorum write only.
6. **ATProto at edge only.** `sh.fidom.*` lexicons versioned. ATProto ingest → `Mailer`
   entity → same trace/crack rules. Never direct sim write.

## 2. Repo layout

```text
sim/      # deterministic: scheduler, movement, deck, net_route, crime, economy, ai
data/     # districts.yaml, jobs.yaml, vulns.yaml, cards.yaml, filters.yaml + schemas
server/   # async runtime, queue, snapshots, ledger store, ATProto bridge
client/   # render + panes (console/status/actions/notify/map), prediction only
tools/    # mapgen validator, balance runner, replay diff, lexicon codegen
docs/design/
```

- `sim` has no dependency on `server/client`. `server` owns time, sockets, DB.
- Chunk format versioned: `chunk_v1`. Snapshot = `{tick, entities, chunks, ledger_seq}` + event log.

## 3. Conventions

- Rust 2024 edition, `clippy -D warnings`, `rustfmt`. Public sim fns documented with
  `/// invariant:` line. Errors as `SimError{kind, entity?, expected, got}` — no panics on bad input.
- Fixed-point money (`i64` cents). Time = `u64` ticks (1 tick = 1s). RNG = seeded `XorShift128+`
  per city, `dice-purpose` tagged for luck UI. Float only for render.
- Intents: `Intent{actor, verb, target, dir, arg}` validated then applied.
  Deltas: `{changed_tiles, sounds, stats, mail}` minimal + full resync on demand.
- Logging: sim emits `Event{ts, kind, actor, target, data}` — sole source for replay, kill-feed, crime evidence.
- Config: `RngSeed`, `UnloadHalo=5x5`, `ActiveTick=3x3`, `CpuBudget`, `TracePerHop` all in `tuning.yaml`, never hardcoded.
- ATProto lexicons in `lexicons/sh.fidom.*.json`, codegen checked in; breaking change = new `v2` record, migrator required.

## 4. Performance / safety budgets (M0 gates)

- Active set: 3×3 sections (2304 tiles) + 1 deck ≤96 cells + net subgraph <200 nodes @60fps iGPU.
- ≤50 full AI active; rest coarse/abstract; sleep minor bots with no observers.
- Snapshot <50MB; chunk swap p95 <50ms; intent→delta p95 <100ms LAN.
- No unwrap/expect in sim hot path; fuzz `Intent` parser in CI.
