# COOLEST — Modern Redesign

> Working title: **FIDOM: Future Internet Domains of Mayhem**
> Tagline: *“Hack to the Future”*
> Sources: `Coolest.odt` (~3400 paras, c.2005, most fleshed out) +
>   `Hacker simulator 2006.odt` (dual-network plot, device/HW model, CLI/UI) +
>   `hacker-game-design.txt` (2024, survival + card/resource Deck)

This directory is a **technical modern redesign** of the original design.
It preserves the core fantasy but re-architects it so it can actually be built,
tested, and shipped in 2026+.

## File index

| File | Topic |
|------|-------|
| `00-evaluation.md` | Honest evaluation of the 20-year-old doc: what was visionary, what was broken |
| `01-vision-pillars-modes.md` | High concept, pillars, the 4 modes redefined |
| `02-physical-world.md` | Surface / interiors, streaming, generation |
| `03-cyberspace-databattle.md` | Computers as dungeons: RAM/disk grids, programs as creatures |
| `04-network-services.md` | Network map, identity, email, banking, authoritative data |
| `05-ai-alignment-crime.md` | NPC scheduling, alignment, crime/record, factions |
| `06-characters-progression.md` | Body/Mind, stats/skills/intrinsics, jobs |
| `07-items-combat.md` | Items, combat resolution, damage model |
| `08-architecture-tech.md` | Engine, ECS, persistence, multiplayer, VR, ATProto |
| `09-ux-roadmap.md` | UX, MVP slicing, milestones, risks |
| `10-synthesis-2006-2024.md` | What the 2006 + 2024 docs add, what was rejected, deltas |
| `11-architecture-standards.md` | Hard rules, layout, budgets — freeze before M0 |
| `12-testing-protocols.md` | CI gates, balance tests, playable gates |

## One-paragraph pitch

A **turn-based open-world roguelike life-sim + hacking-sim**.
You live in a small procedural city (streets, homes, shops, wilderness),
but every computer is also a *place you can enter*: RAM is a tactical grid,
files are rooms, programs are monsters/allies.
You can be a cook, cop, burglar, sysadmin, or hacker clan member —
or ignore plot and run a shop while your daemons defend a music server.
Single-player first, co-op/multi-city capable by architecture.

## Design principles for the rewrite

1. **One sim, four views** — not four games. Physical, Network, Databattle, VR share entity/tick/permission code.
2. **Stream everything** — original 9/13-section idea was right; modernize to chunk streaming + LOD.
3. **Permissions > geometry** — public/private/access-point is the core mechanic, not grid size.
4. **Deterministic sim + dumb clients** — required for save/load, anti-cheat, multiplayer later.
5. **MVP cuts vertically** — one city block + 3 computer types + 5 program types is shippable; original tried to spec everything at once.
