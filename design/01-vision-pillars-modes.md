# 01 — Vision, Pillars, Modes

## High concept

> **Live in a town. Break into its computers. Become what you do.**

You rent a room above a noodle shop. By day you fix scooters.
By night your cracker daemon sits on a laundromat PC waiting for a
bank gateway to hiccup. Cops learn your handle. A clan invites you.
You get a *license to hack* — or you fake one.

Influences: **Caves of Qud** (simulation depth) + **Uplink/Hacknet**
(network fantasy) + **Into the Breach** (readable tactics) + **CDDA**
(living off systems).

## Pillars

1. **Place = power.** Every advantage is positional: stand on the right
   cell (private access, router closet, rooftop microwave) at the right time.
2. **Every system is hackable, nothing is deletable-by-accident.**
   Authoritative records (money, criminal record, licenses) are *mirrored* —
   you can alter the view, not erase truth without cost.
3. **Conduct creates kind.** Alignment is observed behavior + traceability,
   not a character sheet toggle.
4. **Leave the world running.** Programs you leave keep acting on your
   alignment. The city does not pause when you log off a host.
5. **Pressure creates play (2024).** Food/mood decay forces jobs → jobs cost
   mood/CPU → creatures/jobs loop. Survival is the tutorial, not a second game.
6. **Friction is content (2006).** Trace time, logs, tolls, ranges, install
   times, filters — not faster numbers, but meaningful detours.

## Setting / plot (2006 adopted)

Betanet replaced the Internet: gov-regulated wireless + laser backbone,
commercial only — no gaming/filesharing/personal sites/illegal.
Oldnet (old wired Internet) still runs, decaying, hosting exactly that.
Gov hunts Oldnet servers; rival hacker crews hunt each other and you.
You initiate into one crew to keep Oldnet alive, pry open Betanet, and
help neutrals do their own thing. Work double-agent / gov / corp for
alternate runs. Character types: Gov agent, Hacker, Corporate, User,
Power User (+ Gangster/Anarchist post-v1).

## The four modes — redefined (was: physical/network/databattle/virtual)

They are **views on one sim**, switchable with Tab / `>` / `<`:

| # | View | What you see | Core verb |
|---|------|--------------|-----------|
| 1 | **Street** (physical) | Tiles, doors, vehicles, people. Chunk-streamed 2D tilemap with z-levels (floors) | move / use / talk / drive |
| 2 | **Net** (network map) | Graph of known hosts/links, traffic, firewalls, protocol gaps. Fog-of-war | scan / connect / route |
| 3 | **Deck** (databattle, ex-memory grid) | Inside one host: RAM arena + file browser side-by-side. Programs as tokens with head/tail | load / extend / attack / patch / crack |
| 4 | **Veil** (VR, ex-virtual map) | Immersive reskin of Street+Deck rules; death ejects, not permadeath (unless hardcore) | embody / run / transfer |

Original “cannot see inside+outside simultaneously” stays — it’s a
cognitive-load feature, not a limitation. But allow picture-in-picture:
Deck as docked panel while on Street when observing via admin.

## Fantasy loop (30 min)

1. Wake → check messages + jobs board (5 min).
2. Walk block, do honest work or recon (10 min).
3. Pick target host → Net route → Deck infiltration (10 min).
4. Exfiltrate / patch / plant → deal with heat (5 min).
5. Sleep/save; daemons keep working (offline progress capped).

Win states (pick one per run): pay debt, earn license, lead clan,
expose corp, build botnet-free mesh. Lose states: permadeath (classic),
debt spiral, mind-erasure in Veil (hardcore), server ban (multiplayer).

## Non-goals (v1)

- No infinite world. One bounded city: **48×48 sections, 16×16 tiles each**
  (~786k tiles + interiors). Portals to other cities later.
- No real-money trading, no voice chat, no VR-headset requirement
  (Veil is a game mode, not hardware).
- No player-run code execution. “Compiler” is deck-building with
  vetted modules (§03), not Lua/Python eval — kills whole exploit class
  the original `eval string` idea opened.
