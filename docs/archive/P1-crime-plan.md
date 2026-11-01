# P1 Implementation Plan — Wire Crime/Heat (M4)

**Goal:** Make hostile acts carry consequences. `sim/src/crime.rs` is complete and unit-tested, but `World` never instantiates it and `step_npcs` is always handed an empty `wanted` map, so breach/kill/sabotage/cut-power are consequence-free.

### Grounding (observed, not inferred)
- `sim/src/crime.rs`: `Record` (report/mass/standing/alignment/pardon_weak), `Clans`, `License` (add_kill/eligible/abuse), `trace_evidence(base, witness, live_chain)`, `Act{Trespass,Theft,Attack,Murder}` with severities 1/3/4/8, `WARRANT_AT=6`, `LICENSE_KILLS=3`. All tested.
- `sim/src/world.rs:238` `World` has **no** `record`/`clans`/`license` fields. `apply` (line 848) handles `DamageWire`, `DeckLoad`, `DeckStrike`, `VeilSeize`, `SetPower`, `Exploit`, `SendTrojan`, `DeckCard` with no crime reporting.
- `sim/src/world.rs:1386` `self.step_npcs(hour, &BTreeMap::new())` — the wanted map is always empty. `npc_target` (line 640) already implements cop pursuit of `Standing::Wanted` and asserts the `cop_duty` function returns `Pursue`.
- `sim/src/world.rs:613` `step_npcs(&mut self, hour, wanted: &BTreeMap<u64, Standing>)` is the only consumer; `civic::Role::Cop` + `cop_duty` exist.
- `Host::owner` is an `Option` wrapping a `deck::Host` enum (like `WARDEN`), not an `EntityId` (world.rs:2361), so a host has no being-aligned witness today — this is the main design gap for the evidence model.

### Open design decisions (resolve before coding)
1. **Witness for host-based crimes.** `trace_evidence` needs a witness `Alignment` and a `live_chain` bool. For a rig intrusion the natural witness is the rig's defender, but `Host::owner` is an enum. Options: (a) treat a defended/monitored rig as an `Authority` witness unconditionally; (b) extend `Host` to carry an optional `EntityId` owner with a record. Recommend (a) for P1 (smallest change), revisit with (b) if NPC-owned rigs arrive.
2. **`live_chain`.** Define as: the actor still has a live program or unscrubbed log on the target host. `Host::logs_visible(actor)` and `progs` give this. Scrubbed/cold → rumor (×0.1).
3. **Act mapping.** Propose: `DeckLoad`/`Exploit`/`VeilSeize` on a foreign rig → `Trespass`; `DeckStrike` (kill) → `Attack`; `DamageWire`/`SetPower{down}` → `Attack`; `SendTrojan` → `Theft`; hostile `DeckCard` → `Attack`. Confirm against `design/05` (AI/crime) and `design/03`.
4. **Fine vs pardon API.** `pardon_weak(floor)` cannot fully clear (evidence ≥ 0.0 always true). Add `Record::pardon_all()` (or `clear()`) for the pay-to-pardon path.

### Ordered sub-units (one commit each, `area: what and why`)

**P1.1 — World fields + save compatibility** (`sim/src/world.rs`)
- Add three `#[serde(default)]` fields on `World`: `record` as `BTreeMap<u64, Record>`, `clans` as `Clans`, and `license` as `BTreeMap<u64, License>`; init in `new()`.
- Add accessors: `record_of(id) -> &Record`, `standing(id) -> Standing`, `alignment(id) -> Alignment`, `license_of(id) -> &License`, and a `report(...)` writer.
- Keep `SAVE_VERSION = 2` (new fields are `#[serde(default)]`, so v1/v2 saves load unchanged — no migration). Note: `snapshot_hash` will change because `serde_json::to_value` now includes the empty maps; verify `data/golden/` does not pin exact hashes.

**P1.2 — Report crimes from hostile acts** (`sim/src/world.rs`)
- Add `fn report_crime(&mut self, actor, act, at_host, victim)` that builds `Crime { act, at_host, tick, victim, evidence }` via `trace_evidence(1.0, witness, live_chain)` and calls `record.entry(actor.0).or_default().report(crime)`.
- Call it from the hostile arms: `DeckLoad`/`DeckStrike`/`DeckCard` (hostile branch), `Exploit`, `SendTrojan`, `DamageWire`, `SetPower{up:false}`, `VeilSeize`. Non-hostile acts (own rig, `DeckFarm`) stay silent.
- Borrow note: build the `wanted` map and any crime writes from `self.record`/`self.hosts` before the live `beings` borrow, or after its last use, to satisfy the borrow checker.

**P1.3 — Feed `wanted` into `step_npcs`** (`sim/src/world.rs`)
- Add `fn wanted_map(&self) -> BTreeMap<u64, Standing>` scanning `record` for `Standing::Wanted`.
- Replace line 1386 with `self.step_npcs(hour, &self.wanted_map())` (compute into a local first). Audit every other `step_npcs` caller (server runtime) and pass the same map.

**P1.4 — Fines + pay-to-pardon** (`sim/src/world.rs`, `sim/src/crime.rs`, `tools/src/lib.rs`)
- Add `Verb::PayFine` (or `PayFine { amount: i64 }`): requires `Standing::Fined`/`Wanted`, burns the fine through `Ledger` to a civic account, then `pardon_all()`.
- Add `Record::pardon_all()`. Register the verb in `fuzz_verb` (`tools/src/lib.rs`) per convention.

**P1.5 — License abuse on hitting a defender** (`sim/src/world.rs`, `sim/src/crime.rs`)
- On a `DeckStrike`/`DeckCard` kill of a defender program while the actor holds an eligible `License`, call `license.abuse(record, tick)` (revokes + files full-evidence `Attack`).
- Optionally `add_kill()` on threat kills to progress `LICENSE_KILLS`; zone-entry permission is out of P1 scope.

**P1.6 — Surface in GUI/snapshot** (`gui/src/app.rs`, `server/`)
- Show `alignment`/`standing` in the HUD and a wanted indicator; cop pursuit is already simulated, so this is read-only display. `World` serialization carries the new fields automatically through snapshots.

**P1.7 — Tests, fuzz, gate**
- Unit tests: crime reported on `DeckStrike`; `wanted_map` drives cop convergence (extend `cop_pursues_wanted_ignores_clean`); `PayFine` clears record and moves ledger; license abuse revokes; save/load roundtrip with populated records.
- `./gate.sh` (must print `GATE GREEN`, exit 0); `cargo run -q -p tools -- replay-fuzz 1 5000`; `./nightly.sh` if sim evolution/data touched.

### Risks / boundaries
- Evidence model depends on the unresolved witness decision (open question 1); pick (a) and document it.
- `snapshot_hash` shift may affect golden fixtures — check `data/golden/`.
- Determinism preserved: only `BTreeMap`/`Vec` iteration, no `HashMap`, no clock.
- Do not change `SAVE_VERSION` unless a required field cannot be defaulted.

### Verification protocol
```
./gate.sh; echo "exit=$?"          # GATE GREEN, exit 0
cargo run -q -p tools -- replay-fuzz 1 5000
./nightly.sh; echo "exit=$?"        # if sim evolution/data touched
```
Then `git status` clean apart from the intended change.
