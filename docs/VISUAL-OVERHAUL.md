# Visual Overhaul — “Civic Decay / Signal & Static”

> Status: design plan, not implemented. Scope is the `gui/` shell; the
> deterministic sim and server stay unchanged except for a few interaction
> verbs noted in §8.5. The goal is not “more neon”; it is a readable,
> atmospheric game whose information hierarchy survives a real session.

## 1. Experience goals

The player should be able to answer three questions at a glance:

1. **Where am I, and what is around me?**
2. **What is urgent right now?**
3. **What can I do from here?**

Today the shell is an egui default with a long left text column, a top tab
row, ASCII grids, and a console that carries most feedback. It works, but
it reads like a debug harness. The overhaul should make it feel like a
place, a network, and a body under pressure.

**Design pillars**

1. **Two networks, one city.** Oldnet and Betanet should be visible as
   different materials, not just labels.
2. **Glance → inspect → act.** Every important thing has a readable
   summary, a hover detail, and an action surface.
3. **The console is a tool, not the UI.** Logs remain, but urgent state
   and available actions live in the interface.
4. **Pressure is visible.** HP, food, mood, heat, trace, and deadlines
   should be felt in color, shape, and placement.
5. **Craft over chrome.** Thin lines, good type, restrained palette, and
   purposeful motion beat gradients and fake 3D.

## 2. Thematic direction

**Civic decay meets signal decay.**

FIDOM is a city with paperwork, rent, shop counters, and a decaying wired
Internet underneath. The visual language should feel like a municipal
terminal from a future that never quite arrived:

- **Civic layer:** concrete, sodium streetlight amber, photocopied forms,
  transit-map lines, stencil labels, utilitarian panels.
- **Oldnet:** CRT phosphor, oxidized copper, amber/green, scanline
  texture, monospace, imperfect alignment.
- **Betanet:** clean glass, cool cyan, precise grid, sans-serif, laser
  lines, corporate whitespace.
- **Veil:** deep indigo/black, diffuse halos, organic sigils, almost no
  chrome; the interface should feel quieter and stranger.

This is not generic cyberpunk. Avoid the purple-neon-everywhere look.
Color is semantic: amber = Oldnet/streetlight, cyan = Betanet/authority
signal, red = danger/heat, gold = money, mint = recovery/success.

## 3. Visual system

### 3.1 Palette

| Token | Hex | Use |
|---|---|---|
| `ink` | `#0A0E12` | page background |
| `panel` | `#111820` | primary panels |
| `panel-alt` | `#161F29` | raised/hovered panels |
| `line` | `#26323D` | borders, dividers |
| `text` | `#E8EDF2` | primary text |
| `muted` | `#93A1AE` | secondary text |
| `faint` | `#5F6C78` | disabled/ambient |
| `oldnet` | `#F0A830` | Oldnet, streetlight, decay |
| `oldnet-dim` | `#B97A22` | Oldnet secondary |
| `betanet` | `#59D8E6` | Betanet, authority, signal |
| `money` | `#E6B85C` | cash, payouts, prices |
| `danger` | `#E05A5F` | HP loss, heat, wanted |
| `success` | `#62C98A` | healing, completion |
| `veil` | `#A78BFA` | Veil/mindpower |
| `paper` | `#E8E1D0` | civic forms, light-mode accents |

Rules:

- Never use color alone to convey state; pair with icon, label, or shape.
- Oldnet/Betanet colors are functional, not decorative.
- Red is reserved for urgent/danger; don’t use it for ordinary labels.
- Keep large surfaces dark; use color in thin lines, glyphs, and meters.

### 3.2 Typography

Bundle two open fonts (OFL) with local fallbacks:

- **IBM Plex Sans** — UI, names, prose, buttons.
- **IBM Plex Mono** — numbers, addresses, logs, deck/program labels,
  terminal input.
- Optional condensed display face for section labels only; do not add a
  third family unless it clearly earns its bundle size.

Type scale:

| Role | Size | Weight |
|---|---|---|
| Screen title | 20–22 | 600 |
| Section header | 12 uppercase, +0.08em | 600 |
| Body | 14 | 400 |
| Dense metadata | 12 | 400 |
| Numbers/timers | 13 mono tabular | 500 |
| Micro-label | 10 uppercase | 600 |

Rules: no raw `{:?}` debug formatting in player-facing strings; humanize
`Wanted`, `OfficeWS`, `cve-2006-0005`, etc.

### 3.3 Shape, spacing, texture

- 8px spacing base; 4px for dense chips.
- 1px borders; 2px accent line on the active panel/tab.
- 6–8px corner radii, not pill-shaped everything.
- Subtle dither/noise on large panels; scanlines only inside Oldnet/Deck
  surfaces; no constant shimmer.
- Selection: 1px cyan/amber outline + corner ticks, not a filled block.
- Damage/heat: short 120–180ms pulse, then settle. Respect reduced motion.

### 3.4 Iconography

A small custom 16px line-icon set, drawn with egui shapes or bundled SVG:

- needs: food, mood, HP, mindpower
- economy: cash, bank, components (loop/data/algo)
- pressure: heat, wanted, trace, deadline
- world: shop, door, NPC, contract, mail
- deck: kind glyphs for Killer/Shield/Scanner/Cracker/Connector/Cloner/Editor
- net: Oldnet wire, Betanet laser, firewall, power, route

Icons are monochrome by default, semantic-colored only when state changes.
Text labels remain next to unfamiliar icons.

## 4. Shell and information architecture

Replace the current left text dump + top tabs + bottom log with a
**four-zone shell**:

```text
┌──────────────────────────────────────────────────────────────────────┐
│ TOP BAR: handle · day/time · location · cash · heat · notifications  │
├───────────────┬──────────────────────────────────────┬───────────────┤
│ LEFT RAIL     │ CENTER VIEWPORT                      │ INSPECTOR     │
│ vitals        │ Street / Deck / Net / Veil           │ selection     │
│ active        │ view toolbar · map/arena/graph        │ actions       │
│ contract      │ selection · overlays · legend        │ details       │
│ quick actions │                                       │ history       │
├───────────────┴──────────────────────────────────────┴───────────────┤
│ BOTTOM: command input + event ticker (last 3–5, severity-coded)      │
└──────────────────────────────────────────────────────────────────────┘
```

### 4.1 Top bar

- Handle + city/section; day/time; cash (`$12.40`) and bank; heat/wanted
  state; mail/notification count; active view tabs with icons and keys.
- Tabs remain `Street / Deck / Net / Veil` but become **view chips** with
  a 2px underline and keyboard hints, not plain text buttons.
- A “focus mode” toggle hides the rails and enlarges the viewport.

### 4.2 Left rail — “body and obligations”

Ordered by urgency:

1. **Vitals**: HP, food, mood, mindpower as compact bars with numbers and
   icons. Low state gets a label (`starving`, `wanted`, `trace`).
2. **Active contract**: title, objective, time left, Deliver/Abandon; a
   thin deadline bar.
3. **Economy**: cash, bank, components as chips (`2 LOOP`), rations.
4. **Quick actions**: Eat / Watch / Sleep / Revive / Pay fine — only the
   relevant ones, not all buttons always.
5. **Collapsed details**: license, clan, standing; expand on click.

### 4.3 Center viewport

- A view-specific toolbar (top-left) with 3–5 primary actions.
- A selection cursor and hover highlight.
- A legend/minimap (bottom-left or top-right) that can collapse.
- Inline action results (“hack failed: trace active”) near the action,
  not only in the console.
- Map/arena/graph fills the space; do not put long forms inside it.

### 4.4 Right inspector — “what is this, and what can I do?”

Contextual to the selection:

- Selected tile/NPC/host/program/file.
- Name, type, state, key stats.
- Available actions as primary/secondary buttons.
- Tooltips explain formulas and consequences.
- For Deck: kind, max length, CPU, mods, affinity, trace.
- For Net: node card, links, firewall/power, route action.
- For Street: NPC role, shop stock, dialogue, attack/talk/trade.

### 4.5 Bottom ticker

- Command input remains (vi/roguelike identity), with a narrow prompt and
  command history.
- Event ticker shows the last 3–5 events with icons:
  `+$2.20 · contract paid`, `! trace 4/6`, `mail from Eve`.
- Clicking an event opens the relevant inspector/target.
- Full log moves to a drawer/overlay, not the main surface.

## 5. View-by-view redesign

### 5.1 Street — “a city you can read”

**Visual:** top-down tile map, but not a spreadsheet. Walkable floor as
dark concrete with subtle texture; walls as block masses with edge
lighting; doors and shops as distinct glyphs; shop signs in sodium amber;
NPCs as small chips with role icons; player as a directional arrow with a
soft halo. Day/night tint and lit windows. A faint transit-line overlay
shows section exits/ports.

**Interaction:**
- Hover highlights tile + shows tooltip (walkable, shop, NPC, item).
- Click selects; click again on walkable tile queues a path (client sends
  `Move` intents one act at a time). Path preview shows steps and time.
- Context action bar: Talk / Trade / Attack / Pick up / Enter.
- Inspect panel for shops, NPCs, contracts, floor items.
- Keyboard remains first-class: arrows/hjkl, `g` pick up, `a` attack, `t`
  talk, `e` interact.

**Information:** contract board becomes a **posting rail** with cards
(pay/risk/deadline), not a row of long buttons. Active contract appears
in the left rail and as a marker on the map if it has a target.

### 5.2 Deck — “a circuit board under pressure”

**Visual:** RAM grid as a PCB: dark substrate, thin copper traces,
zone/ownership tint. Programs are serpentine tokens with a head glyph and
tail segments; kind glyphs and colors distinguish Killer/Shield/Scanner/
Cracker/Connector/Cloner/Editor. CPU is a power rail with a hard red
ceiling. Trace is a burning fuse/timeline at the top of the arena.

**Interaction:**
- Select a program; inspector shows kind, length, CPU, mods, affinity,
  and actions (Strike/Step/Scan/Crack/Patch/Overwrite/Clone).
- Card hand at the bottom as fanned cards with cost pips and hover rules;
  drag-to-play or click-card-then-cell.
- Loadout bar: kind buttons with CPU/length preview; disabled reasons
  shown, not just a refusal in the log.
- Drag/arrow-step a selected program (requires a small `DeckStep` verb).
- Trace and monitor state are visible at all times, not inferred.

### 5.3 Net — “a map of cables and permissions”

**Visual:** graph canvas, not a list. Oldnet links as sagging copper
cables, Betanet as straight cyan laser lines; nodes as small cards with
arch/kind icons; fog-of-war dims unknown nodes; power cuts render dark
sections; firewalls render as gate icons on links. Route preview animates
packets along the selected path.

**Interaction:**
- Hover node/link for latency/toll/cap/firewall; click for inspector.
- Route planner: source/target chips, “cheapest / safest / fastest”
  presets, step list, total time/toll/trace threshold.
- Actions: Route, Damage wire, Cut/Restore power, Block/Unblock,
  Connect/Probe (Connector).
- Legend for Oldnet/Betanet/gateway/dark/firewall.

### 5.4 Veil — “a quieter, stranger place”

**Visual:** deep indigo field with soft halos; areas as sigils/rooms;
mindpower as a halo/ring around the player; files as glyphs; hazard areas
as slow red/violet bleed. Minimal chrome; type gets more spacious.

**Interaction:**
- Click an exit to move; hover shows hazard and destination.
- Inspector shows host, files, transferred control, mindpower cost.
- Carry/Read/Transfer/Eject as a compact action bar; no wall of buttons.
- Blackout/ejection gets a full-screen but short transition, with a clear
  “what happened” line.

## 6. Interaction model

**Selection grammar:** hover → select → inspect → act. One primary action
per context; secondary actions in the inspector.

**Keyboard-first:**
- Arrows/hjkl movement; Tab cycles views; `1–4` jump views.
- `Ctrl-K` command palette (“crack admin…”, “mail…”, “contract…”).
- `a` attack, `t` talk, `g` pick up, `e` interact, `f` farm, `s` scan,
  `c` crack, `p` patch, `?` help.
- `Esc` closes overlays; `Enter` confirms; focus rings visible.
- Every mouse action has a keyboard equivalent.

**Feedback:**
- Inline result banners near the action, color + text + icon.
- Toasts for asynchronous events (mail, contract expiry) with an action.
- Confirmation for destructive acts (abandon contract, attack friendly,
  spend a large deposit).
- Disabled actions show the reason on hover/under the button.

**No dead ends:**
- Empty board: “No offers today — rest, work, or check mail.”
- Empty Deck cell: “Select a free cell to load; see kind costs.”
- No route: list the blocking link and offer “show on map”.
- Dead player: a single clinic action, not a hidden console command.

## 7. Information presentation rules

1. **Priority ladder:** survival (HP/food/mood) → legal/heat → active
   contract/deadline → economy → exploration. The left rail and alerts
   follow it.
2. **Progressive disclosure:** one-line summary, hover tooltip, inspector
   detail. Never dump formulas into the main view.
3. **Human labels:** no `format!("{:?}")` in UI. “Office workstation,”
   not `OfficeWS`; “Wanted,” not `Wanted`; “Deadline 2h,” not ticks.
4. **Numbers with units and icons:** `$2.20`, `HP 14/20`, `2 LOOP`,
   `Trace 4/6`, `12m`.
5. **Color + shape + label.** Bars for continuous values; chips for costs;
   pips for cards; badges for state.
6. **Time is legible:** deadlines as `2h 14m`, trace as a fuse, contract
   as a bar. Ticks only in debug/tooltips.
7. **Errors are local:** the action that failed explains itself; the
   console keeps the historical record.
8. **One primary action per panel.** Secondary actions sit behind
   “More”/inspector.
9. **No long button rows.** Cards, chips, and wrapped grids; keep rows
   short (existing egui clip trap).

## 8. Technical plan

### 8.1 Module split

Current `gui/src/app.rs` is the whole app and is already ~2.5k lines.
Split before restyling:

```text
gui/src/
  app.rs          // state, wiring, intent dispatch
  shell.rs        // top bar, rails, panels, ticker
  theme.rs        // palette, typography, spacing, visuals
  widgets/        // panel, meter, chip, icon, card, tooltip, palette
  views/
    street.rs
    deck.rs
    net.rs
    veil.rs
  layout.rs       // pure pane geometry + responsive breakpoints
```

Keep `Shell::show` as the frame-free root so the headless harness keeps
working (`egui::Context::run_ui`, accesskit text tree).

### 8.2 Theme and assets

- `theme.rs` exposes tokens and `apply(ctx)` for `FontDefinitions`,
  `Visuals`, `Style`, spacing, and selection colors.
- Bundle IBM Plex Sans/Mono WOFF2/TTF under `gui/assets/fonts/`; include
  licenses; use `include_bytes!` so desktop and wasm are identical.
- Icons: start with a vector `Icon` enum drawn via `Painter`; if a glyph
  set grows, add a small SVG/PNG atlas. Avoid a giant texture.
- No screenshots in tests. Extend the accesskit harness for labels,
  roles, focus, and tooltip text.

### 8.3 Layout

- New shell: top bar 44px; left rail 264px; right inspector 320px;
  bottom ticker 112px; center fills.
- 1280×800 target; 1024×768: inspector becomes a drawer, rails collapse.
- `layout.rs` gains breakpoints and pure rect tests.
- Focus mode: hide rails, expand map, keep a tiny vitals strip.

### 8.4 Performance

- Avoid cloning the whole host/world per frame; snapshot view models.
- Paint the map/graph with `egui::Painter`; cull off-view items.
- Cache text layouts for static labels; keep tooltips lazy.
- 60fps on integrated GPU at 1280×800; wasm bundle stays reasonable.

### 8.5 Sim interaction additions (small, optional)

The visual pass should not change game rules. These are the only likely
sim/server additions:

- `Verb::DeckStep { x, y, to_x, to_y }` (or selected-program arrow step)
  so the Deck can support drag/arrow movement.
- Client-side click-to-move emits existing `Verb::Move` intents one act at
  a time; no new sim verb required initially, but a `MoveTo` path preview
  may be added later.
- Nothing else: selection, hover, inspector, cards, and palettes are
  client state.

## 9. Phased implementation

Each phase is shippable and keeps the game playable.

### UI-1 — Foundation (shell, theme, palette)
1. `gui: theme tokens, bundled fonts, visuals`
2. `gui: shell top bar + left rail + right inspector + ticker skeleton`
3. `gui: command palette (Ctrl-K) and keyboard focus model`
4. `gui: shared widgets (panel, meter, chip, icon, tooltip)`
5. `gui: move existing status/actions into the new shell`
6. Tests: layout breakpoints, accesskit labels, focus/palette smoke.

### UI-2 — Street
7. `gui: street tile painter (materials, doors, shops, NPCs, selection)`
8. `gui: hover tooltip + click-to-inspect + path preview`
9. `gui: context action bar (talk/trade/attack/pickup/interact)`
10. `gui: contract posting rail + active contract card`
11. `gui: day/night tint + section minimap`
12. Tests: hover/selection/action labels; no overflow at 1024/1280.

### UI-3 — Deck
13. `gui: deck arena painter (PCB grid, kind glyphs, head/tail glow)`
14. `gui: CPU rail + trace fuse + host header`
15. `gui: program inspector (kind/mods/affinity/actions)`
16. `gui: card hand with cost pips + preview`
17. `gui: loadout bar with disabled reasons`
18. `sim: DeckStep` + GUI drag/arrow movement
19. Tests: kind actions, card preview, trace/CPU readability.

### UI-4 — Net
20. `gui: net graph painter (Oldnet cables, Betanet lasers, node cards)`
21. `gui: fog-of-war/power/firewall visuals`
22. `gui: route planner presets + packet preview`
23. `gui: node inspector + actions`
24. Tests: route/power/firewall interactions through clicks.

### UI-5 — Veil
25. `gui: veil area sigils, mind halo, hazard bleed`
26. `gui: file glyphs + transfer/read inspector`
27. `gui: ejection/blackout transition`
28. Tests: enter/move/transfer/carry/read/eject.

### UI-6 — Polish and accessibility
29. `gui: motion pass (reduced-motion toggle)`
30. `gui: contrast/colorblind audit + focus rings`
31. `gui: responsive drawer at 1024`
32. `gui: help/legend overlay and empty states`
33. Docs/player guide + screenshots-in-docs only (not tests).

## 10. Acceptance criteria

- At 1280×800 and 1024×768: no clipped controls, no overlapping panes,
  every primary action reachable by mouse and keyboard.
- A new player can answer “where am I / what is urgent / what can I do”
  in under 5 seconds without reading the console.
- All four views share the same shell grammar; each has a distinct
  material identity.
- Every state color has a text/icon label; contrast meets WCAG AA for
  text; reduced-motion toggle works.
- Headless accesskit tests cover the new interaction paths; no image
  snapshot tests.
- `./gate.sh` and `./nightly.sh` stay green; no sim regression.
- 60fps target on the integrated-GPU budget; wasm shell loads offline
  fonts and icons.

## 11. Risks and non-goals

| Risk | Mitigation |
|---|---|
| Scope creep into a full custom engine | Stay in egui; use Painter + tokens; no 3D, no isometric projection |
| Visual noise obscures state | Priority ladder, one primary action, restrained palette |
| Accessibility regressions | Labels/roles stay; contrast and keyboard audits each phase |
| Headless tests become brittle | Stable labels, role-based queries, no screenshots |
| Fonts/bundle bloat | Two OFL families, subset if needed; local fallbacks |
| Performance | View-model snapshots, Painter culling, no per-frame world clones |
| Theming delays gameplay | Ship UI-1/UI-2 first; Deck/Net/Veil restyle in parallel with content |

**Non-goals:** isometric/3D city, animated character art, voice acting,
mobile-first layout, a full icon atlas before the shell is stable, or
changing game rules to fit the UI.

## 12. Wireframes

See:

- `docs/diagrams/ui-street-wireframe.svg`
- `docs/diagrams/ui-deck-wireframe.svg`

They are intentionally structural: panels, hierarchy, and interaction
surfaces, not final art.
