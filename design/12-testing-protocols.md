# 12 — Testing Protocols (run before/after every slice)

## 0. Principle

No slice merges without its gate green. Balance tests are tests — TTK, trace,
ledger drift fail the build like a crash.

## 1. CI stages

```text
lint(fmt, clippy -D warnings, yamllint, jsonschema data/) →
unit(sim: scheduler, deck verbs, route cost, crime evidence) →
property(proptest: damage soak ≥0, ledger closed, no double-bounce loop) →
snapshot(replay determinism: same seed+intents == same hash) →
worldgen(100 seeds: BFS reachability to guarantees, road/river port match) →
soak(100 bots × 10k ticks: no deadlock, no panic, AI ≤ budget) →
balance(TTK 3-4, crack N-turns, trace vs bounce curve) →
save/load(roundtrip + version migrate v1→v2) →
atproto(lexicon validate, ingest via Mailer only, no direct write)
```

- `main` blocked on all. Nightly adds 1000-seed gen + 1M-tick soak + chaos (kill PDS, drop power node).

## 2. Required suites per system

| System | Must-test | Gate |
|--------|-----------|------|
| Scheduler | energy order, Speed>100 bonus, frozen-chunk catch-up decay | order vector exact |
| Street gen | guarantees reachable, doors paired, edge blocks explicit | 100/100 seeds pass |
| Deck | strike/takeover/overwrite/clone/crack, headshot kills, tail trim, CPU starve → glitch → crash | TTK table in range |
| Cards/resources (2024) | paired-stat next-turn expiry, produce×rate, consume/consume-all scaling | no infinite loop |
| Net/trace (2006) | hop cost, log append/scrub, tracer vs scrambler, tolls, min-modem cap, gateway required | trace curve monotonic |
| Ledger | mint/burn/transfer, ATM IOU reconcile, zombie/spam caps | `Σbal == mint-burn` always |
| Crime | trace→record→warrant→pursuit, license grant/revoke on abuse | evidence threshold stable |
| AI | memory cap 7+5, fear vector, timetable home/work, ambient-only NPCs (no hunger) | no O(n²), ≤50 full |
| Save | snapshot+log replay hash equal, holder-before-component, migrate | hash equal |
| ATProto | handle/DID resolve, `sh.fidom.*` validate, offline queue | no sim bypass |

## 3. Playable gates (human)

- **M0.5 survival:** survive 3 days, buy firewall. Fail if median >25 min or <10 min.
- **M2 deck:** 60% infiltration success first try with starter deck; 90% after loadout change (proves choice matters).
- **M3 net:** player can articulate cheapest vs safest route (5-sec test).
- Record funnel: `started → first job → first hack → first trace → survived`. Drop >40% at any step = block next slice.

## 4. Test data + tools

- `tools/replay`: `seed + intents → hash`; `tools/balance`: sweep CPU/trace/TTK to CSV;
  `tools/mapgen-check`: render ports + BFS failures; `tools/ledger-audit`: recompute from log.
- Golden files: `data/golden/{deck_ttk.csv, trace_curve.csv, gen_100.json}` — diff fails on drift.
- Manual checklist per release: dual-console focus, map zooms World→Live, click-to-walk, actions pane trace-time visible, save on power-loss in Veil.

## 5. Definition of merge-ready

Green CI + updated `tuning.yaml` + golden diff approved + 15-min playthrough video/link + no new `// TODO(balance)` without issue link.
