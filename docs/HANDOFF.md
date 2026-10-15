# FIDOM — Agent Handoff (status + todo)

> Written 2026-10-15 for the next agent. Branch `main`, head `28074af`,
> 194 tests green, `./gate.sh` and `./nightly.sh` both green, tree clean.
> Read `README.md` and `PLAYER_GUIDE.md` first for the game itself; this
> file is the engineering state and the traps.

## 1. What this is

A turn-based open-world roguelike life-sim + hacking-sim, iterated from
design docs from 2005/2006/2024. One bounded city, four views
(Street / Net / Deck / Veil). Design docs `design/00-12` are the spec of
record; `design/09-ux-roadmap.md` has the M0–M5 slices and
`design/12-testing-protocols.md` the gates.

## 2. Workspace layout

```
sim/         deterministic game sim (no clock/net/fs/threads) — the only place gameplay mutates
  src/world.rs   World, Intent, Verb, apply() — the act pipeline (the hot file)
  src/deck.rs    Host arena: programs, CPU, vulns, resources, cards, dice, retaliation
  src/net.rs     NetGraph: nodes, links, routing, firewalls, tolls
  src/power.rs   PowerGrid: section blackouts, blackout-aware routing
  src/crime.rs   Record / Clans / License (COMPLETE but UNWIRED — see TODO)
  src/mailer.rs  Mailer + ZombieNet (zombie net is now wired; mailboxes are not)
  src/veil.rs    Veil areas, mindpower, transfer/eject
  src/{ledger,shop,talk,street,civic,data,renderer,portal,scheduler,error,ids,pos}.rs
server/      authoritative shell: submit/step/apply_one, snapshots, intent log, replay, TCP runtime
gui/         eframe shell (desktop + wasm). app.rs is the whole app; render.rs tiny-skia headless shapes
client-web/  headless demo binary (scripted tour) — proves the sim is UI-agnostic
tools/       balance/gen/soak/chaos/replay/mapgen/ledger-audit + fuzz/replay-fuzz/fuzz-sweep
data/        tuning.yaml jobs.yaml cards.yaml vulns.yaml districts.yaml dialogues.yaml golden/
lexicons/    ATProto sh.fidom.{mail,board,profile}.json
```

## 3. Commands you will use constantly

```bash
export SCCACHE_DISABLE=1 RUSTC_WRAPPER=""   # WSL: a Windows sccache in PATH breaks builds
./gate.sh            # fmt + clippy -D warnings + tests + lexicons + hygiene  (must be green before commit)
./nightly.sh         # gen 1000, mapgen 200, soak 1M, chaos 100k, fuzz-sweep 25x20k
cargo run -q -p client-web                 # scripted demo tour
cargo run -q -p gui                        # desktop shell (needs a display)
cd gui && trunk serve                      # wasm shell (must run from gui/, not the workspace root)
cargo run -q -p tools -- fuzz 1 20000      # invariant sweep
cargo run -q -p tools -- replay-fuzz 1 5000
cargo run -q -p tools -- fuzz-sweep 25 20000
```

Always check `./gate.sh`'s exit status directly. Piping it
(`./gate.sh | tail`) hides the failure because `tail` exits 0 — this bit a
previous session and a broken commit slipped through.

## 4. Conventions that are load-bearing (do not break these)

1. **All gameplay mutation goes through `sim::World::apply(&Intent)`.**
   Clients (gui, server, tools) never mutate the world directly — they
   build an `Intent` and hand it to `Server::apply_one`. Queries stay
   direct (`world.host(..)`, `net.route(..)`, `power.powered_route(..)`).
   This is why Deck/Net/Veil/Street are all replayable now. If you add a
   mechanic, add a `Verb` variant.
2. **Every processed intent is logged, accepted or refused.**
   `Server::apply_one` (server/src/lib.rs:62) pushes to `self.log` even on
   `Err`, because a refusal still jumps the world clock to the actor's due
   tick (and charges upkeep). Replaying only the accepted intents diverged
   from the live world — that was a real bug found by fuzzing. `step()`
   still returns results only for accepted intents.
3. **Determinism is a hard requirement.** No wall clock, no `HashMap`
   iteration in gameplay, no unseeded RNG. Dice live on `Host.rng`
   (xorshift, serialized) and advance per roll. Production and NPC stepping
   happen *inside* `apply`, so replay reproduces them. `tools replay-fuzz`
   is the regression net — run it when you touch `apply`.
4. **Save compatibility.** New `World`/`Host` fields must be
   `#[serde(default)]` (or `#[serde(default = "..")]`) so old snapshots
   still load; `sim/src/world.rs` has `SAVE_VERSION` + v1→v2 migration.
   Never reorder/retag existing serialized fields. `data/golden/*` (TTK,
   trace, gen) fail on drift — if a golden changes, be sure the change is
   intended.
5. **Data is validated, not decorative.** `sim/src/data.rs` `parse_*`
   functions reject bad content (unknown district, job resource, vuln
   archetype/effect, duplicate ids). Cards/vulns/jobs ids are cross-checked
   against `implemented_cards()` / archetypes by tests. If you add data
   fields, validate them.
6. **One commit per unit**, message style `area: what and why`, gate green
   before each commit, new behaviour ships with a test.

## 5. Gotchas discovered the hard way

- **Producers are not intruders.** `Host::retaliate` skips maker programs
  (passive resource creatures), and `DeckFarm` draws no retaliation beat.
  Before this, farming on the defended demo rig spawned the warden adjacent
  and it killed your producer on the same beat.
- **Reflect hits the attacker's body, not the head.** A head hit is
  unconditionally lethal, which made reflect a one-shot kill.
- **Headshots kill; tail hits strip segments.** Most damage tests must
  target a tail cell (`p.tail.back()`) or they measure a kill instead of
  damage.
- **A load adjacent to the warden dies in the same act** (retaliation runs
  inside `apply`). `Host::free_cell_for` avoids hostile neighbours; that is
  why the console `farm` uses it.
- **`Vec<u8>`/`u8` underflow bites.** `(tick - 1)` panicked in a test when
  tick was 0. Prefer `saturating_sub`.
- **egui `Panel`/`Grid` row overflow silently disables clicks.** Five long
  job buttons in one row pushed `Sleep` outside the clip rect and its clicks
  were ignored (test still "found" the button). Use `horizontal_wrapped`
  and keep rows short.

## 6. Headless UI testing (no screenshots — important)

Screenshots blew up a previous session's context. Do not add image-based
UI tests. Instead `gui/src/app.rs` has a harness:

- `Shell::show(&mut self, ui)` (app.rs:448) is the frame-free UI root;
  `eframe::App::ui` just calls it.
- `mod headless` (app.rs:1247) drives it via `egui::Context::run_ui` and
  reads the **accesskit** text tree. Helpers: `Harness::new`,
  `pass_with(events)`, `click(label)` (re-resolves the centre per pass),
  `click_last(label)`, `click_prefix(prefix)`, `click_at(pos)`,
  `tap_key(key)`, `has(role,label)`, `has_text(substr)`, `labels()`.
- Traps: static text is in `node.value()`, buttons in `node.label()`
  (`text_of` handles it); `out.textures_delta.clear()` is required or epaint
  panics; accesskit is enabled with `ctx.enable_accesskit()`.
- Labels with **live counts** change after each click — use `click_prefix`,
  not `click`, for those (e.g. `Code loop (n)`, `Work 0: .. [loop n/4]`).
- A click that adds a log line shifts layout; that is why `click`
  re-resolves the position before the release pass.

## 7. What is done (recent units, newest first)

- **Cards**: resource costs (design/03 consume-to-fire) and per-program
  `Mod` abilities (`Verb::DeckMod`) that fire on every exchange.
- **Crime/heat wiring (M4)**: `World` carries `record`/`clans`/`license`,
  illicit acts file evidence-weighted crimes (witness + live-chain gated),
  `standing()` feeds a wanted map into `step_npcs` so cops pursue, and
  `Verb::PayFine` settles the record. Clans/license remain unwired.
- `Exploit` + `SendTrojan` verbs and capped daily spam income (vuln →
  breach → zombie net → ledger).
- `ScrubLog` + `SetFirewall` verbs (get-out-clean trail erasure; transit
  firewall filters now reachable from play).
- `HostArch` archetype-scoped vulns, `breach` vs `privilege` effects,
  `Host::logs_visible`; vulns.yaml 3 → 10 entries, validated.
- `tools fuzz` / `replay-fuzz` / `fuzz-sweep` + nightly sweep.
- Job unit requirements (4–6 loop/data/algo) with manual `Code` and
  passive `DeckFarm` producers; bank balance shown; Net view renders the
  graph and power state; full 8/8/4 card table with deterministic dice;
  all four views routed through the server; headless egui harness.

## 8. TODO — prioritised

### P1 — Wire crime/heat (M4). DONE (see section 7).
Remaining sub-items, all smaller:
  - **Clans**: `Clans::join/is_enemy/friendly` exist but nothing joins or
    checks them in play; needs a `Verb::JoinClan` (fee + initiation) and a
    check when attacking a clanmate (`mark_enemy`).
  - **License**: `License::eligible`/`abuse` exist; wire "threat kills"
    (e.g. killing a warden program while licensed) and `abuse` when hitting
    a defender, plus a `Verb::ApplyLicense`.
  - **Fines → Wanted escalation**: today any mass > 0 is `Fined` and
    >= 6 is `Wanted`; consider an unpaid-fine deadline before the warrant.

### P2 — Card/program abilities. DONE (see section 7).
Costs and per-program mods both landed. Remaining nicety: nothing consumes
`Fx`/`Mod` from a program's own *type* yet, so the design's "type synergy"
(producers that match the attacker's type fire harder) is unbuilt.

### P3 — Smaller, well-scoped
- **Mailboxes / email trojan delivery**: `ZombieNet` is wired, but
  `Mailbox`/`Mailer` are not on `World`. A real trojan needs recipients that
  open mail (NPC behaviour). Currently `SendTrojan` requires an existing
  breach.
- **GUI polish**: Deck Net mini-map (design/09 says Deck shows RAM + files +
  Net minimap together); `Ctrl-K` command palette; `>`/`<` stairs; diagonal
  vi-keys (`h`/`j`/`k`/`l` and arrows move today, `y`/`u`/`b`/`n` do not).
- **`docs/deferred-wasm.md`** still leads with the superseded 0.21 problem;
  the "Resolved" section is the truth.
- **client-web** is a scripted demo, not a network client. `server::net` and
  `server::runtime` (tokio) exist and are tested but nothing in the GUI uses
  them — that is the multiplayer seam (design/08).

### Blocked / external
- **ATProto PDS relay.** Lexicons validate (`server/src/atproto.rs`) but
  there is no PDS bridge, so nightly's PDS chaos is skipped and noted. Do
  not fake it.

## 9. Verification checklist before you commit

```bash
export SCCACHE_DISABLE=1 RUSTC_WRAPPER=""
./gate.sh; echo "exit=$?"        # must print GATE GREEN and exit 0
cargo run -q -p tools -- replay-fuzz 1 5000     # if you touched apply/state
./nightly.sh; echo "exit=$?"     # if you touched sim evolution or data
```
Then `git status` should be clean apart from your intended change. If you
add a `Verb`, also add it to the fuzz generator in `tools/src/lib.rs`
(`fuzz_verb`) so it stays replay-checked, and bump the roll range there.
