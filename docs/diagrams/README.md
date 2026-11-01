# Diagrams

Generated from `generate_diagrams.py`; no third-party dependencies.

```bash
python3 docs/diagrams/generate_diagrams.py
```

- `core-architecture.svg` — clients, authoritative server/sim, bridge,
  PDS edge, and the trust boundary.
- `gameplay-loop.svg` — local turn loop, job-board accept subloop, and the
  asynchronous ATProto edge.
- `ui-street-wireframe.svg` — four-zone shell, Street map, inspector,
  contract/vitals rail, and ticker.
- `ui-deck-wireframe.svg` — CPU rail, RAM arena, trace fuse, program
  inspector, card hand, and ticker.

Architecture diagrams are embedded in `docs/ARCHITECTURE-PROPOSED.md`;
UI wireframes are embedded in `docs/VISUAL-OVERHAUL.md`.
