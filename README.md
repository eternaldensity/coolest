# FIDOM — Future Internet Domains of Mayhem

> *Hack to the Future.* A turn-based open-world roguelike life-sim + hacking-sim,
> iterated from design docs dating back to 2005.

You live in a procedural city split between decaying wired **Oldnet** and
regulated wireless **Betanet**. Every computer is a dungeon: RAM is a tactical
grid, files are rooms, programs are monsters and allies. Eat, work, hack,
dodge traces — become what you do.

## Status

Playable sim + headless client + GUI shell (184 tests green).

- `cargo test --workspace` — full suite
- `cargo run -p client-web` — scripted tour of all systems

## Start here

- [SETUP.md](SETUP.md) — toolchain, build, test, layout, workflow
- [PLAYER_GUIDE.md](PLAYER_GUIDE.md) — how the game plays today
- [design/](design/) — full design docs (00–12): vision, world, deck combat,
  network, AI/crime, characters, architecture, roadmap, and the
  2006 + 2024 synthesis

## One loop

Work a job → stay fed and cheerful → route a hack across both nets →
fight programs in the Deck → scrub your logs → beat the trace →
spend the money before the cops connect the dots.
