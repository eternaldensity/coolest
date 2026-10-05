# 05 — Simulation, AI, Alignment, Crime

## Tick model (replaces per-computer turn order essays)

- Single energy scheduler (§02). Programs tick as entities with `cost = base / cpu_share`.
- LOD: `Full` (active chunks) → `Coarse` (halo: move only, no combat) → `Abstract` (off-map: timetable advance + ledger only). Unattended programs run `Coarse` AI on owner’s alignment.
- No “move all admins on your turn” exploit: admin programs are independent entities; `control` grants orders, not extra moves.

## NPC cognition (replaces universal-AI + infinite memory)

Utility AI, 5 layers (kept from original, bounded):

```text
purpose (timetable + mission) → alignment filter → observe (capped) → evaluate (threat/opportunity) → act (weighted)
```

- **Working memory cap:** 7 entities + 5 recent events + home/work/known-food. Forgetting curve + room-change flush. No lifetime-everything log.
- **2006 scope cut (load-bearing):** NPCs do **not** sim hunger/mood. They `randomly buy/use` per agenda + ambient traffic + reactions + personal info for computer interactions. Only the **player** runs full food/mood (2024). Saves tick + memory, kills 2005’s “everyone starves” spiral.
- **Danger model:** per-enemy `dmg_to_me, dmg_i_deal, speed, range, allies_near`. Naive HP-sum removed (fixes flame-suit vs flamethrower error).
- **Flee vector:** separate fear field; direction = weighted away-from-threat + toward-exit/ally. Original “flee direction unsolved” now solved by flow field.
- **Traits:** `obedience, daring, greed, aggression` in [-1,1], data-driven per job.
- Group AI: leader emits `Task{verb,target,reward}`; members bid by trait/distance. Separation → act alone. Attacking a group tags you `enemy_of[group]` for its lifetime.
- Cast (2006): `gov_agent (locate/disrupt/arrest/destroy), hacker (keep Oldnet + pry Betanet + block rivals), corporate (serve + report), user (buy + report), power_user (illegal use, no hack, neutral-but-bustable)`. Double-agent across crews allowed; each has agenda + ambient + reaction + player-specific hooks.

### Timetables (was: 6 slices of day)

```yaml
shopkeeper:
  06-10: open_shop
  10-14: serve
  14-18: serve/restock
  18-22: leisure:club|bar
  22-06: home:sleep
cop:
  shifts: [day|night], patrol: 2 zones, off_duty_still_cop: true
```

Data files, hot-reloadable. Misplaced NPCs path home; auto-restock shops; doctors remember cures via shared `instinct` table, individuals learn prices/uses.

## Alignment (was: 7-way taxonomy)

Kept as behavior contracts, enforced by AI + permissions:

| Align | Street rule | Deck rule |
|-------|-------------|-----------|
| `honest_passive` | never trespass, retreat when hit | stays out of Private, no crackers/scanners |
| `honest_active` | as above, but defends | defends, chases intruder off host |
| `untrusted` | small piracy/hosting, no break-in | may host grey files, helps vs evil if present |
| `authority` | pursues threats/dishonest/evil, may enter Private in pursuit | allowed everywhere when on duty + trace |
| `minor_dishonest` | opportunistic hacks, clan-aided | pokes Private, flees authority |
| `major_dishonest` | hunts banks/military, fights authority | attacks non-clan on sight (except evil) |
| `evil` | takeover attempts, weird mods | attacks all (viruses only if hit first) |

- NPC alignment static (except plot flip); **player alignment dynamic** from ledger of traced acts.
- Clans/gangs: `no_attack + mutual_aid` within tag; join via `fee + initiation mission`.
- Admin programs inherit host owner alignment; if host booted/logged as admin by attacker, admin flips to attacker until reboot + password rotation.

### Trace / crime / license (was: criminal record + license to hack)

```rust
struct Crime { act: ActKind, at_host, at_tile, time, victim, evidence: f32 }
Trace: if honest/authority observes illicit act in Deck/Street and you still have live chain → evidence links handle → `at` computer → physical identity.
```

- Record lives in `LedgerService` mirrored (§04). Seriousness → fine / arrest warrant / cop dispatch.
- Player alignment = function(record, recent acts). Editing record changes effective alignment — but audit daemons watch.
- **License to Hack:** earned by `threat_kills + clean_record + quals`. Grants: enter honest/authority Private to fight intruders without trespass flag. Abuse (hit defender) = trace + revoke + heavy penalty — enforced, not honor system.
- Remote-admin-capture defense: don’t fight owned admins head-on; crack `permissions` file to revoke their control instead. Taught in tutorial heist.

## Offline / unattended behavior

Any program left running when you disconnect keeps ticking `Coarse` on your alignment. Locally-run code is untraceable (only `Connector` heads are traceable). Trojans run on planter alignment. Viruses/worms run on no-one.
