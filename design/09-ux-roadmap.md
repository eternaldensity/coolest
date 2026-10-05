# 09 — UX & Roadmap

## UX (fixes 52-letter / implicit-hands pain)

- Keep roguelike keys (`hjklyubn`, `>`, `<`, Tab views) + full mouse + search palette (`Ctrl-K`: “crack admin …”, “mail …”).
- Letters as **shortcuts**, not capacity: `a-zA-Z` auto-assign + pin; overflow in scroll list + filter. No 50-item cell cap; paginate.
- Deck shows RAM + files + Net mini-map simultaneously (original “exclusive Net map” removed; PiP allowed).
- Hands/belt/containers visualized: paper-doll + `needs 2 hands` badges; illegal moves preview in red with reason.
- Hearing/talk: one conversation focus + queue; `ignore list`; long messages show `…(3s)` talk time.
- Tutorials: 1) open shop, 2) cable a laptop, 3) crack guest → private buffer lesson, 4) license heist (revoke remote admin via permissions, not DPS).

## MVP slicing (vertical, playable at each; 2024 survival first)

- **M0 Engine:** walker + 1 section + 1 floor + save/load + scheduler.
- **M0.5 Survival playable (2024, informs rest):** 1 room + cheapest PC/net/firewall,
  small bank, no income, player-only food/mood decay, order food/meds online,
  shows/cute-videos prop mood, 2 programming jobs (manual slow vs creature fast).
  Success = survive 3 days + buy better firewall. Metrics: time-to-broke, job/mood tradeoff.
  Locks needs model (player-only) + economy sinks before Deck tuning.
- **M1 Street slice:** 3×3 sections (residential+shop+park), 5 jobs, day/night, cops.
  Vehicles = fast-travel to named nodes only (2006 cut); NPCs ambient only, no hunger.
- **M2 Deck slice:** `HomePC + OfficeWS + Router`, paired stats + 2024 card table (8 neg/8 pos/4 manip),
  3 resource types, `Tracer/Scrambler/Monitor/MemoryViewer`, vuln/exploit/patch ×3, zombie/spam capped.
- **M3 Net slice:** Oldnet (wired/decay) + Betanet (wireless range + laser tolls),
  bounce/log scrub, firewall filters, email trojan, transfer cap = min(modems).
- **M4 Crime slice:** trace/record/warrant, clans, license mission, double-agent option.
- **M5 Veil slice:** 2 areas, mind transfer, eject/death rules.
- Later: houses/hotels/trains/planes, cameras, multi-city, co-op.

Each milestone has TTK/funnel metrics + bot soak test (100 bots × 10k ticks, no deadlock, no ledger drift).

## Risks & mitigations

| Risk | Mitigation |
|------|------------|
| Deck balance unfun (turtle/camp) | CPU-starve giants, access-point buffer, playtest TTK gates |
| Griefing via unattended programs | Coarse AI caps, no-trace-local rule, opt-in PvP shards, audit daemons |
| Save corruption (original fear) | Snapshot + event log + BFS reachability tests on gen |
| Scope relapse (trains/planes/boats…) | Feature-flagged, post-1.0; cut list explicit |
| Content moderation (crime jobs) | Abstracted, victimless or consenting targets; reporting + single-player filters |

## Definition of done (v1.0)

One bounded city, four views working, 8 jobs, 12 Deck mods, email+bank+record mirrored, license arc, Veil intro, 10h story + endless sandbox, deterministic saves, headless server binary. Trains can wait.
