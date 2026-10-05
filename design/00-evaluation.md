# 00 — Evaluation of the Original (~2004) Design

## What it was

A ~337KB / 3478-paragraph spec for a Java roguelike attempting:
`ADOM × GTA × Uplink × The Sims × MUD`.
Four interlocked maps: **physical / network / databattle / virtual(VR)**,
plus economy, email, banking, crime records, gangs, train schedules,
farming, cooking, bionics, and player-programmable computers.

## What was visionary (keep)

1. **Computers as dungeons.** RAM + disk as grids, `head/tail` programs,
   public/private/access-point cells, CPU as carrying capacity.
   This predates *Uplink, Quadrilateral Cowboy, Hacknet* popularizing it.
   The `head must be targeted for takeover`, `overwrite tails not heads`,
   `clone segment` mechanics are genuinely tactical.

2. **Dual-anchor persistence.** “A computer seen on net must be anchored
   in physical space” — i.e. no orphan nodes. Correct instinct; modern
   equivalent is bidirectional entity references with lazy resolution.

3. **Streaming with GUIDs.** `Sect,-4,32`, `computer'<cell-guid>`,
   hash-resolver with `loaded/not-loaded + priority + LRU unload`,
   pseudo-rooms for ungenerated content. This is 2004-speak for
   chunk streaming + asset handles. Fundamentally sound.

4. **Alignment as AI, not just reputation.** honest / untrusted / authority /
   minor-dishonest / major-dishonest / evil + clan non-aggression,
   license-to-hack, trace-to-`at`-computer. Anticipates modern
   systemic wanted/heat systems.

5. **Day-slice scheduling.** 2AM-6AM / 6AM-10AM / … roles with
   home→work→leisure→sleep loops. Right; needs data-driven timetables now.

6. **Player-as-program.** Program-PC containing full Mind, mind-transfer in VR,
   trojans running on your alignment when unattended. Ambitious but unique.

## What was broken (fix in rewrite)

| Problem | Example in original | Modern fix |
|---------|---------------------|------------|
| **No MVP / no vertical slice** | Trains, planes, farming, chemistry, music, boating all spec’d before movement works | §09 roadmap: 1 block + 3 computer archetypes first |
| **Mechanics without numbers** | Dodge “up to 20 blows/sec”, recoil, PV/DV, catch, aim-angle — no formulas, “Um? Whips?” | §07 gives dice + to-hit + TTK budgets |
| **Persistence over-engineering** | Circular linked-webs, `saved-flag` two-pass, GUID copy semantics, “hash-pointer 1=2 rejected” | ECS + integer EntityIDs + chunk save + event log (§08) |
| **Turn model incoherent** | Physical turns + per-computer turns + “computer you are at moves first” + VR real-time | Unified energy-clock (§08): 1s base tick, programs cost CPU-time |
| **UI frozen in 1980s** | 52 letters, “only 50 things per cell”, implicit hands/belt state machine | Keep hotkeys, add search/filter/loadout UI; letters as *shortcut*, not capacity limit (§09) |
| **Scope contradictions** | “No infinite wander” vs “keep getting harder further out”; “never save in multiplayer” vs “evacuate city” | Bounded city (e.g. 64×64 sections) + portals to other cities/servers (§02, §08) |
| **Security/economy hand-waving** | “Money must be transferred from somewhere” but no mint/sink; “fake banking program with hack abilities” | Ledger as authoritative service, double-entry, §04 |
| **AI unsolvable as stated** | Universal AI weighing every direction/action + remembering every observation forever → O(n²) + memory leak | Utility AI with capped working memory + LOD (§05) |
| **Accessibility / ethics blind spots** | Pickpocket/murder/kidnapper as grind jobs, no consent model | Fictions + opt-in crime servers, robust reporting (§09) |

## Verdict

**Keep the fantasy, kill the implementation plan.**
The original is 80% worldbuilding, 20% engine, 0% shippable loop.
The rewrite below keeps ~70% of systems but re-expresses them as:
data-driven defs + streaming chunks + deterministic sim + permission checks.

Riskiest carry-overs: databattle balance, unattended-program griefing,
VR recursion. All gated behind feature flags in §09.
