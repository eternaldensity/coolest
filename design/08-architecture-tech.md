# 08 — Architecture & Tech

## Stack (opinionated, replaceable)

- **Sim:** Rust (deterministic, `serde` snapshots) or Go. No gameplay `eval`.
- **Render/clients:** Bevy / Godot / web (egui) — dumb, predictive, resync from server log.
- **Net:** QUIC/WebSocket + protobuf; single incoming queue; Twisted-style reactor retained as idea, modern async runtime.
- **DB:** SQLite per city (dev) → Postgres (prod) for ledger + entity snapshots + event log.
- **AI:** behavior trees + utility scoring in sim, not in client.

## ECS sketch

```rust
// entities are u64; components plain data
struct Pos { city, sx, sy, z, x, y } | HostPos { host, x, y } | HeldBy(EntityId)
struct Health { hp, max_hp, pv, dv }
struct Mind { align, traits, timetable, memory: BoundedVec<Event, 12> }
struct ProgramComp { owner, head, tail, mods, cpu }
struct HostComp { ip, arch, cpu_mips, ram_grid, drive_grids }
struct NetLink { from_host, to_host, bw_total, bw_used, proto, up: bool }
```

Systems: `Scheduler, Movement, Vision, DeckCombat, NetRoute, PowerGrid, Economy, CrimeTrace, Timetable, Decay`.

- Original `PhysicalObject > Item/Character`, `Attackable/Openable/Portal/Cell` hierarchies → components + systems. No deep inheritance.
- `GUID` class → `EntityId u64` + `TileId` math + `HostId`. String form debug-only.
- Save: `snapshot = { tick, entities, chunks, ledger_seq }` + `event_log` for replay. Load: create-all then link-all (keeps original two-pass insight).

## Determinism & anti-cheat

- Sim RNG: `XorShift128+` per city, seeded; dice helper exposes purpose for luck UI.
- Server authoritative; clients send `Intent{verb, target, dir}`; server returns `Delta{changed_tiles, sounds, stats}`. Clients cache static art, never trust HP.
- Translations/abbreviations client-side, versioned at login.

## Multiplayer (was: central server + bots + Jabber, now ATProto)

Phase 1 single-player. Architecture ready:

- Cities = shards/servers. No inter-city sim traffic; only `post/email + traveler carry` (cheap, async). Portal travel = character export/import with `city + counter` identity (not pointer `id()`).
- NPCs = server bots (same `Intent` path as players). Thrown/burning/rusting = event-queue items, not fake clients.
- Lag: `wait = max(5ms, 2×ping)` example kept as floor, plus interacting-group detection; slow room to slowest *in group*, not whole city.
- Chat split in two layers:
  - **In-sim positional `say`**: tick-synced, gated by tile/loudness/hearing/walls. Never leaves sim; hackable via bugs/cameras/logs. No external protocol — latency + eavesdrop rules make ATProto unsuitable here.
  - **Persistent/federated `mail/post/profile`**: ATProto (DID `did:plc`, handle, PDS + Relay + AppView). Player handle = ATProto handle; cross-city mail, job board, clan announcements, global character search go through PDS records (`sh.fidom.*` lexicons). Out-of-game client can read/post; in-game `Mailer+` can hack/mirror them.
- No game-network bypass: ATProto messages never inject sim effects directly; they ingest via `Mailer` entity with the same trace/crack rules as internal mail.

## CLI + UI panes + map zooms (2006 adopted)

- CLI verbs: `ls/cd/mkdir/search/mv/view/delete/upload/download/help`,
  `logon` local vs `login` remote + `logoff/leave`,
  `select/connect/login/logout/disconnect/show connections/show logins`
  (hop-count args, no double-bounce same host). Auto-suggest addrs. Multi-CLI
  frames allowed (2006) — shared HW/CPU, extra frames close on `leave`; one device focus at a time.
- Panes (single-window): console bottom, status left (time/device/score/health/money/inventory tabs),
  actions centre-top (procs + mem/cpu/time-left + trace-time), notify top-right (mail/delivery),
  map top-right. Map enlarges when off-device; actions hides.
- Map zooms: World → Continent → Region → City → Live (FOV-limited, click-to-walk, click-char to follow).
  City shows devices + static features (laptops hidden unless carried-visible); Live adds objects/chars.
  Click-centre / double-zoom-in / right-zoom-out. Arrows stay in console (2006 focus warning).

## VR / Veil tech

- Veil host = `radio_tower + big_host` running 5-10 areas; each area = one section/domain labeled by host IP.
- Entry via helmet → `MindToken` with full Mind copy; inventory = mind-files (single-use, delete-after-view, capacity by mindpower). No real-item duping: disk-clone erases source.
- Power loss: blackout → gradual deletion externals-first + warnings; reboot refills different content but preserves hidden links (farming hazard kept as feature).
- Island renumber on join/split: `O(n)` renumber smaller side (kept from original analysis; hash-pointer and O(n²) refcount rejected).

## Performance budgets

- Active 2304 street tiles + 1 deck (≤96 cells) + net subgraph (<200 nodes) @ 60fps on integrated GPU.
- AI: ≤50 full agents active; rest coarse/abstract. Auto-sleep minor bots with no observers.
- Save: incremental chunk dirty flags; full city snapshot <50MB.
