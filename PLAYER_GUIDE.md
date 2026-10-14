# New Player Guide — FIDOM: Future Internet Domains of Mayhem

> Status: sim + headless client playable via `cargo run -p client-web`;
> desktop shell via `cargo run -p gui`; web shell via `cd gui && trunk serve`
> (Street/Deck/Net views browser-verified 2026-10-06).

## The pitch

Live in a town. Break into its computers. Become what you do.
Two networks span the city: decaying wired **Oldnet** (free, illegal) and
regulated wireless **Betanet** (commercial, watched, toll roads). You start
broke with the cheapest rig and eat, work, and hack your way up.

## Your first minutes (the survival loop)

You wake with **food 80 / mood 70 / $2.00 / 3 rations**. Every action ticks
both bars down:

- **Work** a programming job → earns cash, costs mood + food.
- **Eat** a ration → +30 food. No rations? Order more (online stores, M1).
- **Watch** a show → +15 mood, costs $0.30.
- At **zero food or mood, jobs lock out**; starving doubles how long
  everything takes. This is the whole early game: stay fed and cheerful
  enough to keep earning.

## The four views (Tab to switch, when the GUI lands)

| View | What | Verbs |
|------|------|-------|
| Street | Tile streets, doors, shops | move / use / talk |
| Net | Host graph, latency, tolls | scan / connect / route |
| Deck | Inside one computer: RAM arena + files | load / move / strike / crack / patch |
| Veil | VR overlay of the same rules | embody / transfer |

## Hacking 101

1. **Net:** pick a route. More hops = slower + pricier (tolls) but harder to
   trace (`trace threshold = 4 + 2×hops`). Laser backbone is fast and
   unbreakable but charges per hop. Crossing Oldnet↔Betanet needs a gateway.
2. **Deck:** load programs onto free cells. Programs have a **head** (moves)
   and **tail** (hit points). Hit the tail to shrink, hit the head to kill.
   Watch **CPU**: over budget blocks new loads and growth. World host 1 is
   defended: your loads draw a **warden** that hunts them and a **trace** —
   strike fast, and scrub or outrun it.
3. **Get out clean:** every hop logged you. Go back and **scrub** each log
   (needs one of your programs on that host), or outrun the **trace** —
   scramblers halve its speed.
4. **Mind the monitor:** armed hosts flag exploits and traces. Patch your way
   in with single-use exploits; a patched hole stays shut.

## Heat, clans, license

- Crimes witnessed by honest eyes **with your connection live** count full;
  rumors barely count. Fines → **Wanted** at severity 6. Cops pursue Wanted
  even off shift.
- Your **alignment** (Honest → Untrusted → Dishonest → Evil) is computed from
  your record — editing records changes who the city thinks you are.
- **Clans** offer backup but demand a fee + initiation; hitting a clanmate
  makes you its enemy for life.
- **License to hack** (3 threat kills + spotless record) lets you enter
  honest machines to fight intruders legally. Hit a defender and it is
  revoked, plus a fresh charge.

## Dying (or not)

- Street: standard roguelike stakes.
- Deck: losing a program just costs the program.
- Veil: your mind enters at 5+ mindpower; death **ejects** you with heavy
  mindpower drain instead of killing you. Power blackouts eject everyone.

## Controls today

```bash
cargo run -p client-web   # scripted tour of all systems
cargo test --workspace     # prove the rules hold (89 tests)
```

Keys (`hjklyubn`, `>`, `<`, Tab, `Ctrl-K` palette) and the full pane layout
(console/status/actions/notify/map) arrive with the GUI shell.
See `design/09-ux-roadmap.md` for what lands in which milestone.
