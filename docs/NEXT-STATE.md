# NEXT STATE — v0.4 “Embodied City”

> **Status 2026-11-01: implemented.** Body/Mind, health/revival, items/
> inventory, street combat, cop arrest, and the real-mindpower Veil all
> landed; `./gate.sh` green (228 test executions) and `./nightly.sh`
> green. The sections below are the plan as written; see `HANDOFF.md` for
> the current engineering state and the v0.5 shortlist.
>
> Original baseline: `main` @ `ba6c0db`, `bash ./gate.sh` green
> (exit 0), 201 test executions (176 unique `#[test]` fns; `gui` compiles
> its app twice), tree clean except the stale untracked
> `docs/P1-crime-plan.md`.
>
> This document is a *plan*, not a status claim. It records what is
> actually built (verified against source), what is missing, and the next
> major implementation arc.

## 1. Verified current state

| Area | Status | Evidence |
|------|--------|----------|
| Sim / server / replay | Solid | `sim` 14.6k LOC total workspace; every mutation through `World::apply`; `Server::apply_one` logs refusals; v1→v2 save migration; replay-fuzz + nightly |
| Street life sim | Working | needs, 5 jobs with component gates, Code/Farm, shops/hours, rent, sleep, talk, bank/pocket, 16×16 sections, NPC timetables, cop pursuit |
| Deck | Working, shallow | arena, CPU, programs head/tail, cards + resources + mods, vulns, trace/scrub, warden, mail/trojans |
| Net | Working | graph, routes, tolls/latency, fog, damaged wires, power blackout reroute, firewall filters |
| Crime / heat | Wired (M4 complete) | `record`/`clans`/`license` fields; `report_crime` at world.rs:657; `wanted_map` at :781; `PayFine`, `ApplyLicense`, `JoinClan` |
| Veil | Skeleton only | `veil.rs` library is tested, but only 4 world verbs; area 1 and host `EntityId(70)` hardcoded (world.rs:1430–1438); GUI passes constant `mp: 8` (app.rs:229) |
| Server / networking | Built, unused | sync + tokio TCP servers, snapshots, log replay; no client uses them |
| ATProto | Blocked | lexicons validate; no PDS bridge; nightly skips PDS chaos |

### What is *not* built (largest gaps)

1. **Characters / body / progression (design/06).** `Being` is only
   `pos, next_at, speed, food, mood, money, rations, last_rent_day`
   (world.rs:217). No HP, PV/DV, stats, skills, XP, mindpower, conscience.
2. **Items / inventory / street combat (design/07).** No item module, no
   inventory, no Attack verb, no street damage. `PLAYER_GUIDE.md` already
   says “Street: standard roguelike stakes” — that is currently false.
   Cops can pursue but cannot arrest or hurt; heat ends only at `PayFine`.
3. **Program identity (design/03).** `Program` has id/owner/head/tail/
   max_len/cpu_per_seg/mods only (deck.rs:180). `DeckLoad` always installs
   the same generic killer (world.rs:1356). No Cracker/Scanner/Connector/
   Shield/Cloner/Editor kinds, no loadout choice, no type synergy. The M2
   playable gate (“choice matters”) cannot pass yet.
4. **Hosts as dungeons.** RAM cells are a flat arena: no
   Public/Shared/Private/Access zones, no accounts/passwords, no files or
   drives, no crack/patch/overwrite/takeover/clone verbs. The “computer is
   a dungeon” pillar is only half present.
5. **Veil as a real space (M5).** No transfer/file/damage verbs, no
   host-bound areas, no blackout hook, no hazard. Mindpower is a GUI
   constant, not a being stat.
6. **Content and UX.** 5 jobs (v1 target 8), no tutorial/story arc,
   no Ctrl-K palette, no Deck Net mini-map, no `>`/`<` stairs, no diagonal
   movement. Mail eavesdrop (`Mailer::snoop`) has no verb; attachments are
   a boolean, not a file.

### Housekeeping found during the audit

- **`./gate.sh` and `./nightly.sh` are mode 100644** (not executable), so
  the handoff’s `./gate.sh` fails with exit 126. Use `bash ./gate.sh` or
  commit the executable bit.
- `docs/HANDOFF.md` is stale: head `9582bdd`, “crime COMPLETE but
  UNWIRED” (it is wired), and P1/P2 still listed as todo.
- `docs/P1-crime-plan.md` is untracked and entirely superseded. Decide
  whether to delete it or move it to `docs/archive/`.

## 2. Candidate next states

| Option | Scope | Impact | Risk |
|--------|-------|--------|------|
| **A. M5 Veil close** | transfer/files/damage/blackout, real mindpower, area content, GUI | Completes roadmap milestone; small | Low |
| **B. M2.5 Programs with identity** | program kinds, loadout, scan/crack/zones/files/accounts, type synergy | Deepens the game’s unique Deck pillar; fixes M2 gate | Medium/high |
| **C. M6 Embodied City** | Body/Mind, HP, items/inventory, street combat/arrest/death, then fold M5 in | Makes it a roguelike life-sim, gives M4 heat teeth, unlocks progression/content | High but staged |
| **D. v0.4 polish/content** | 3 more jobs, palette, minimap, stairs, tutorial/objectives | Perceived completeness | Medium; polishes thin systems |

**Recommendation: C, with M5 closed inside it.** Body/Mind is the
foundation Veil needs, so this is not a detour around M5. It fixes the
largest zero-code design area (06/07), turns the existing crime/cop/
license systems into real consequences, and creates the hooks for skills,
loot, and a story. Program identity (B) is the next state after this one.

## 3. Target: v0.4 “Embodied City”

**Theme:** the player is a body in the city, not just a cursor with cash.
Every view shares one health/mind model, and hostile acts can now cost
something.

**Exit criteria**

- Player has HP/PV/DV and can be damaged, killed, and revived; meds heal.
- At least one melee weapon, one ranged weapon, one armor, and consumables
  are data-driven, buyable, lootable, equippable, usable, droppable.
- Street combat resolves with the design/07 d20 formula (single HP pool;
  body parts deferred), NPCs react/flee/fight, and cops can arrest or
  fight a Wanted player.
- Veil uses the player’s real mindpower/HP: transfer, mind-files, hazard
  damage, eject-on-death, and power-blackout ejection all work through
  verbs and the server.
- Every new act is replay-deterministic, save-compatible, fuzz-covered,
  and has headless GUI tests. `gate.sh` + replay-fuzz + nightly green.
- `client-web` gains a scripted “fight → arrest → veil” tour.

**Explicitly out of scope for v0.4:** 18 body parts, all 40 skills,
permadeath-by-default, program kinds/zones/files/accounts, multiplayer
clients, ATProto, vehicles, story arc.

## 4. Ordered workstreams (one commit per unit)

### W0 — Baseline hygiene
1. `chore: fix script modes and refresh handoff baseline`
   - `chmod +x gate.sh nightly.sh` (or document `bash`), update HEAD/test
     count/“crime wired” in HANDOFF, decide fate of `P1-crime-plan.md`.
   - Acceptance: `./gate.sh` works from a clean shell.

### W1 — Body & Mind foundation (`sim/src/world.rs`, new `sim/src/body.rs`)
2. `sim: add Body/Mind components behind save defaults`
   - `Body { hp, max_hp, pv, dv, str, dex, spd, tough }`,
     `Mind { mindpower, comp_literacy, conscience }`.
   - Keep NPC needs exemption; NPCs get HP/stats via the same health map.
   - Decision: store health in `World.health: BTreeMap<u64, Health>`
     (`#[serde(default)]`) so PCs and NPCs share damage code; keep
     Body/Mind on `Being` for player progression.
   - Save: bump `SAVE_VERSION` to 3 and migrate v2 → health/rng defaults
     (or prove defaults suffice; either way test old snapshots).
3. `sim: damage, death, and clinic revival`
   - Saturating damage; death event; revival at home with a medical burn
     and debt (default), not save deletion. Hardcore toggle later.
   - Accessors for views: `health(id)`, `is_alive`, `hp_fraction`.
   - Tests: zero-HP, overkill, revival, old-save load, no underflow.

### W2 — Items & inventory (`sim/src/items.rs`, `data/items.yaml`, `sim/src/data.rs`)
4. `data: item definitions and validation`
   - `ItemDef { id, name, glyph, weight_g, hands_req, slot, dv, damage,
     ranged, effect }`; parse/validate unknown slots/effects, duplicate
     ids, non-positive weights.
5. `sim: inventory, hands, weight, equip/use/drop`
   - `Inventory { stacks: BTreeMap<String, ItemStack>, worn: BTreeMap<Slot, String> }`
     on `Being`; weight cap from STR; hands requirement; no per-instance
     durability in v0.4.
   - Verbs `Pickup`, `Drop`, `Equip`, `Use`; `Buy` stocks item ids.
   - Shop lines and loot table move to item ids; ledger closure preserved.
6. `gui: inventory panel and item actions`
   - Worn/held/carried lists, weight, Use/Equip/Drop; use
     `horizontal_wrapped` to avoid the egui clip trap.
   - Headless tests: buy → equip → use → drop, weight refusal.

### W3 — Street danger (`sim/src/combat.rs`, world.rs, civic.rs)
7. `sim: deterministic world RNG for street resolution`
   - Add serialized `World.rng` seeded from `seed`; dice helper. Keep
     host dice independent. Check `data/golden/*`; update only with intent.
8. `sim: melee/ranged Attack verb with PV/DV`
   - d20 + skill/stat vs 10 + PV, crit/fumble, damage − DV, HP loss;
     range/line/cover basics. Single HP pool; no body parts.
   - NPCs get attack/defend utility; civilians flee; cops fight Wanted.
9. `sim: cop arrest and surrender`
   - Adjacent cop + Wanted: demand surrender; `Verb::Surrender` pays the
     fine or jails to morning (cash/contraband seized). Attacking the cop
     is resistance and escalates. No soft-lock: jail always releases.
10. `sim: death, loot, and witness reactions`
    - Drop inventory on death/NPC kill; report Attack/Murder through the
      existing `report_crime`; clans/cops react.
11. `gui: combat HUD, target selection, flee/surrender`
    - Adjacent-target buttons, HP bars, hit/miss log; headless tests for
      attack, flee, arrest, revival.

### W4 — Close M5 Veil on the new Body/Mind (`sim/src/veil.rs`, world.rs, gui)
12. `sim: bind veil areas to hosts, use real mindpower`
    - Remove `mp` from `VeilEnter`; read Mind.mindpower. Replace hardcoded
      area 1 / `EntityId(70)` with a seeded complex registered by the
      world/demo (radio tower + host).
13. `sim: veil transfer, files, hazard damage, blackout`
    - Verbs `VeilTransfer { host }`, `VeilCarry { file }`,
      `VeilRead { index }`; area hazard deals mind damage on move; 0 HP
      ejects with `DEATH_DRAIN`; `SetPower` blackout ejects anchored veil
      users. <5 mindpower refuses re-entry until restored.
14. `gui: veil targets, files, hazard warning, real mp/hp`
    - Headless full flow: transfer, read file, take hazard, eject, blackout.

### W5 — Integration, content, docs
15. `tools: extend fuzz_verb and replay coverage for every new verb`
16. `client-web: scripted fight → arrest → veil tour`
17. `docs: refresh PLAYER_GUIDE/HANDOFF/README; record v0.4 metrics`
18. Human gate: survive 3 days, win and lose a fight, get arrested and
    recover, complete one Veil transfer, save/load mid-fight.

## 5. Design decisions to settle before W1

1. **Death model:** clinic revival + medical debt (recommended) vs
   permadeath. Revival keeps the sandbox learnable; hardcore is a flag.
2. **Health storage:** shared `World.health` map (recommended) vs fields on
   `Being`/`Npc`. The map avoids duplicating damage logic and supports
   future entities.
3. **Combat granularity:** one HP pool in v0.4; body parts later. The
   design’s 18-part model is a stretch goal, not a launch gate.
4. **Arrest:** explicit `Surrender` verb (recommended) vs automatic
   arrest on adjacency. Explicit keeps agency and testability.
5. **RNG:** world RNG for street (recommended) vs reuse host 1’s dice.
   Street and Deck must not share a stream or one perturbs the other.
6. **Save version:** bump to 3 if RNG/health cannot be safely defaulted;
   never reorder existing serialized fields.
7. **Items:** def-id stacks (recommended) vs per-instance entities.
   Stacks are enough for v0.4 and keep snapshots small; durability is a
   later state.
8. **Cop lethality:** cops should prefer arrest/stun over murder; tune so
   a Wanted player can escape, not auto-die.

## 6. Risks and mitigations

| Risk | Mitigation |
|------|------------|
| Save incompatibility (Being/Npc/World changes) | `#[serde(default)]`, v2→v3 migration, roundtrip test with populated v2 JSON |
| RNG changes replay/goldens | Serialize RNG, seed from `world.seed`, run replay-fuzz + nightly, update goldens only deliberately |
| Combat balance turns the early game into a death spiral | Tune via `tuning.yaml` + tools/balance; clinic revival; cops prefer arrest |
| Scope creep (body parts, 40 skills, 8 jobs) | v0.4 exit criteria fixed above; body parts and full skill tree deferred |
| Item data drift | `parse_items` validation + cross-check against `data/items.yaml` like cards/vulns |
| egui row overflow hides clicks | `horizontal_wrapped`, short rows, headless accesskit tests |
| Cop AI soft-lock/jail loop | Jail always releases at 06:00; debt capped; test no-progress case |

## 7. Verification protocol

```bash
export SCCACHE_DISABLE=1 RUSTC_WRAPPER=""
bash ./gate.sh; echo "exit=$?"                 # GATE GREEN, 0
cargo run -q -p tools -- replay-fuzz 1 5000    # after any apply/state change
bash ./nightly.sh; echo "exit=$?"              # after sim evolution/data changes
cargo run -q -p client-web                     # scripted tour still passes
```

Every new `Verb` also goes into `tools/src/lib.rs::fuzz_verb` and the roll
range is bumped, per the existing convention.

## 8. Immediate next five commits

1. `chore: fix script modes and refresh handoff baseline`
2. `sim: Body/Mind + shared health (save v3)`
3. `sim: damage, death, clinic revival`
4. `data: item definitions + validation`
5. `sim: inventory pickup/equip/use/drop`

After those, W3 (combat/arrest) is the first unit that changes how the
game *plays*; W4 closes M5; W5 makes the state demonstrable.

## 9. Deferred to the following state (v0.5)

- **Programs with identity (M2.5):** Killer/Shield/Scanner/Cracker/
  Connector/Cloner/Editor kinds, loadout choice, type synergy, zones/
  files/accounts, crack/patch/overwrite/takeover/clone verbs.
- Full skill tree and job ranks; 8 jobs; story/tutorial arc.
- Ctrl-K palette, Deck Net mini-map, stairs/portals, diagonal movement.
- Real network client (GUI over `server::runtime`) and ATProto bridge
  (still blocked externally).
