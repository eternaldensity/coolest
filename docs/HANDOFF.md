# FIDOM — Agent Handoff (status + todo)

> Refreshed 2026-11-01 for v0.5/v0.6. Branch `main`, 264 test executions
> (211 unique `#[test]` fns), `./gate.sh` and `./nightly.sh` green. Read
> `README.md` and `PLAYER_GUIDE.md` first for the game itself;
> `docs/NEXT-STATE.md` is the v0.4 history, `docs/V0.5-V0.6-PLAN.md` is
> the current roadmap, and this file is the engineering state and traps.

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
  src/crime.rs   Record / Clans / License (wired: crimes, pursuit, fines, license)
  src/body.rs    Body / Mind / shared HP + clinic revival (v0.4)
  src/items.rs   item defs, inventory, worn gear (v0.4)
  src/combat.rs  pure d20/PV/DV street resolution (v0.4)
  src/mailer.rs  Mailer + ZombieNet (zombie net wired; snoop still has no verb)
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

- **v0.6 Programs with Identity (M2.5 core)**: Killer/Shield/Scanner/
  Cracker/Connector/Cloner/Editor kinds, `Program.kind`, kind stats from
  `data/kinds.yaml`, `DeckLoadKind`, `DeckScan`, `DeckCrack`, `DeckPatch`,
  `DeckOverwrite`, `DeckClone`, `DeckConnect` (routed remote probe), and
  kind-matched `DeckFarmKind` producers (+1 affinity yield). Scanner has a
  scan report; old programs default to Killer. The remaining v0.6 item is
  the human M2 60%/90% playtest gate.
- **v0.5 Contracts & Consequence**: local daily board, `AcceptOffer`,
  escrow, one active contract, delivery/hack/courier/bounty completion,
  `DeliverContract`, `AbandonContract`, expiry, employer reputation, and
  the Street board/tracker. `data/contracts.yaml` is validated and mirrored
  by code defaults.
- **v0.4 Embodied City (design/06-07, plus M5 close)**: shared HP/health,
  Body/Mind, clinic revival, data-driven items + inventory + worn gear,
  shop gear, d20/PV/DV street combat, NPC aggro/loot, cop attacks,
  surrender-or-jail, real-mindpower Veil with host-bound areas, mind-files,
  hazards, and power-blackout ejection. New verbs: `Attack`, `Surrender`,
  `Pickup`, `Drop`, `Equip`, `Use`, `Revive`, `VeilTransfer`, `VeilCarry`,
  `VeilRead`; `VeilEnter` no longer takes `mp`.
- **Mail/trojans (design/03/04)**: per-entity mailboxes, `Mailer+`
  crafting, `SendMail`/`SendTrojan`/`OpenMail`, NPCs reading mail on their
  turn, and trojan-on-open enslaving the reader's hosts.
- **Cards**: resource costs (design/03 consume-to-fire) and per-program
  `Mod` abilities (`Verb::DeckMod`) that fire on every exchange.
- **Crime/heat wiring (M4, complete)**: `World` carries
  `record`/`clans`/`license`; illicit acts file evidence-weighted crimes
  (witness + live-chain gated); `standing()` feeds a wanted map into
  `step_npcs` so cops pursue; `Verb::PayFine` settles the record;
  `Verb::ApplyLicense` grants the license on 3 threat kills + clean
  record with abuse revocation; `Verb::JoinClan` pays dues and marks
  clanmate enmity.
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

v0.4 (Body/Mind, items, street combat, M5 Veil) is done; see section 7.
The current roadmap is `docs/V0.5-V0.6-PLAN.md`.

### v0.5 — Contracts & Consequence. DONE.
Local board, escrow, deadlines, completion, abandon/expiry, reputation.
See `docs/JOB-BOARD-ACCEPT.md`.

### v0.6 — Programs with Identity. CORE DONE.
Kinds, loadout, scan/crack/patch/overwrite/clone, Connector probe,
producer affinity. Still open: the human M2 60%/90% success-rate
playtest. Host zones/files/accounts are the leading v0.7 candidate; PDS
bridge is also v0.7+ (requirements ready in
`docs/PDS-BRIDGE-REQUIREMENTS.md`).

### Later
- PDS Bridge Mode A: operated PDS + polling; then firehose/OAuth.
- Hosts as dungeons: zones, files, drives, accounts, remote admin.
- Content/UX: 8 jobs, tutorial/story, Deck Net mini-map, Ctrl-K, stairs,
  diagonals.
- Body-part combat, full skill tree/XP, clan mutual-aid bonuses.
- `client-web` is a scripted demo, not a network client. `server::net` and
  `server::runtime` (tokio) exist and are tested but nothing in the GUI
  uses them — that is the multiplayer seam (design/08).

### Blocked / external
- **ATProto PDS bridge.** Lexicons validate (`server/src/atproto.rs`) but
  there is no PDS transport, identity, auth, or queue, so nightly's PDS
  chaos is skipped and noted. Do not fake it. The requirements and OSS
  shortlist are in `docs/PDS-BRIDGE-REQUIREMENTS.md`; start with P0
  (Mode A: operated PDS + polling) before firehose/federation.

## 9. Verification checklist before you commit

```bash
export SCCACHE_DISABLE=1 RUSTC_WRAPPER=""
./gate.sh; echo "exit=$?"        # must print GATE GREEN and exit 0
cargo run -q -p tools -- fuzz 1 20000
cargo run -q -p tools -- replay-fuzz 1 5000     # if you touched apply/state
./nightly.sh; echo "exit=$?"     # if you touched sim evolution or data
cargo run -q -p client-web       # scripted survival/items/fight/arrest/veil tour
```
Then `git status` should be clean apart from your intended change. If you
add a `Verb`, also add it to the fuzz generator in `tools/src/lib.rs`
(`fuzz_verb`) so it stays replay-checked, and bump the roll range there.
