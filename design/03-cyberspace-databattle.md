# 03 — Cyberspace: Deck (Databattle)

> Original insight preserved: *a computer is a dungeon you walk with programs.*

## Host model

```rust
struct Host {
  id: HostId, ip: [u16;3], // major.local.device — Café Infinity owns a /16
  arch: HostArch,          // HomePC | OfficeWS | Server | Bank | Military | Router | IoT | Laptop
  cpu_mips: u32,           // budget, not speed gate
  ram: Grid<MemCell>,      // e.g. 12x8, zones colored
  drives: Vec<Grid<DiskCell>>,
  accounts: Vec<Account>,  // guest sandboxes + admin
  autorun: Vec<ProgramId>,
  power_section: SectionId,
}
enum MemZone { Public, Shared, Private, AccessPublic, AccessPrivate }
```

- **RAM arena** and **file browser** shown side-by-side. Selecting any
  segment shows `total_len / which_segment / known_caps`.
- Files >1 sector must have all sectors present in cwd to run/open.
- `Shared` is world-readable, admin-writable. Guest homes are
  `public but passworded`. Everything else `Private` (only visible from
  `AccessPrivate` cell).
- Hidden/invisible: `sees_hidden`, `stealth` flags — no magic.

## Programs as creatures

```rust
struct Program {
  id: EntityId,
  owner: EntityId,          // controller; trojans store true owner
  align: Alignment,         // synced per-tick to owner unless trojan/virus
  head: Vec2, tail: VecDeque<Vec2>,
  len: u8, max_len: u8, moves: u8,
  mods: Vec<ModId>,        // deck-built abilities
  cpu_per_seg: u8,
}
```

- Moves **4-dir** (keeps Deck readable vs 8-dir Street).
- Head moves/extends; over-long tail auto-trims; head can back over own body.
- Cannot enter occupied cell except via attack mods.
- Starter set (MVP): `Cracker, Scanner, Connector, Killer, Shield, Cloner, Editor`.
  Full set adds `Takeover, Overwriter, Glitcher, Antivirus, Mailer, Miner, Tracer, Scrambler, Monitor, MemoryViewer`.

### Trace / bounce / logs (2006 adopted)

- Every hop `+trace_time`; each transit host appends `Log{who,when,via}`.
- `Tracer` on victim/defender counts down; `Scrambler/Monitor` slows or false-flags it.
  Viewing trace needs `Tracer` running; confusing it needs `Scrambler`.
- `Monitor + behaviour/connection filters` auto-flags `initiate trace / mass login / spam burst`.
- Bounce across N hosts multiplies safety but costs `latency + tolls + log surface`.
  Cross Oldnet↔Betanet bounce needs `Gateway` HW+SW. No re-bounce same host on one chain
  (prevents loop inflation); multi-outbound allows re-hit via other path.
- Log deletion is a Deck verb (needs Private cell + Editor), not a menu toggle —
  you must go back and scrub each hop.

### Core verbs

| Verb | Target | Cost | Note |
|------|--------|------|------|
| `load/run` | free Public cell | cpu + 1 act | from disk or remote upload |
| `extend/move` | adjacent free | 1 move | length gates abilities |
| `strike` | enemy segment | 1 act | deletes 1 tail seg; headshots kill/takeover |
| `takeover` | enemy head | 2 acts, mindpower check | ownership flip, not copy |
| `overwrite` | enemy tail | 1 act | sever + isolate |
| `clone` | self segment → free cell | 2 acts + cpu | child inherits controller |
| `crack` | password gate | N turns | multi-turn, interruptible |
| `edit/decompile` | file | 1 act + skill | decompile yields 1 random mod ≤ level cap, preserves nerfs (no infinite speed loop) |

### CPU / Glitch (replaces 88%/100%/121% cliffs)

- Load `L = Σ initial + per_seg`. Headroom `H = cpu_mips - L`.
- `H<0`: soft degradation — +glitch dice per act, growth blocked, then
  random crash to free pressure. No hard locks.
- Glitch table (d100): wrong target / weak hit / skip move / self-lock 1t.
  Configurable per `HostArch` (`glitch_prone` tag).

### Connection chaining (simplified from original)

Old rule “one branch only” kept, but made explicit:

- `Connector` on `AccessPublic` → open Net view → pick target → spawn
  `Connector:head` in target’s free Public cell. Tail stays home.
- If home connector dies/disconnects, remote head **crashes** (visible, lootable).
- Private routing same, but remote lands in Public cell and sees Private graph.
- One active outbound chain per user by default; `Router` + `Multihome` mod
  lifts to 3. Kills tree-explosion complexity the original feared.

### Malware + vuln/exploit/patch (2006)

- `Virus`: uncontrolled, attacks anything incl. other viruses. Controlled only
  in the tick it was planted by planter if planter present.
- `Worm`: virus + `Connector`.
- `Trojan`: runs on planter’s alignment when executed by victim. Primary vector:
  email attachment via `Mailer` (needs `Mailer+` to craft).
- `LogicBomb`: on-read trigger: crash reader / eat segment / grid glitch.
- `Zombie`: compromised host sending spam/relaying for owner; early illicit income,
  capped per host/day so ledger doesn’t inflate.
- `Antivirus` needs fresh `sigdb` file — itself infectable/deletable. Deliberate tension.
- Vulns: `HostArch × SW version → VulnId`; `Exploit` fires once per vuln, `Patch`
  closes it. `NetworkTester` locates wire damage; `MemoryViewer` shows remote procs.
  Larger vuln table than 2005’s generic cracker, but data-driven (`vulns.yaml`).

### Deck stats + cards + resources (2024 adopted)

Grid position stays; resolution uses paired stats + dice + resources:

- Derived Deck stats: `Dedication/Sway, Focus/Distract, Stability/Disturb,
  Courage/Fear, Clarity/Confuse` ← from `comp_literacy/mindpower/toughness/patches`.
  Attacks target negative pole, **next-turn only**.
- Closed effect table (v1): skip-card if `focus∤2/3`, Stability DoT, ban die N,
  random-target (stacking), −N to rolls, decoy zero, dmg-per-roll-used, downgrade;
  heal-lowest, +N, extra roll, tank/reflect/splash-half, cancel/avoid,
  load/copy/skip/upgrade. Small enough to balance TTK 3-4 hits.
- Resources: `optimized loop / data mgmt / advanced algo` (+ more). Jobs require N;
  manual slow, creatures auto-produce faster + free player, higher tier = faster
  but hungrier for CPU/RAM, 1-2 types each. In Deck: some cards **produce**
  (×rate if creature type matches), some **consume** to fire, some **consume-all**
  to scale. Forces deck/type synergy instead of one best Killer.

### Remote presence rule

You are `at` one host. You control programs only at `at`-host or via
live chain head. Everything else runs on stored alignment (see §05).
No remote puppetry without a live tail — fixes original “control from anywhere” hole.
