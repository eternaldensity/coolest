# Diagrams

Generated from `generate_diagrams.py`; no third-party dependencies.

```bash
python3 docs/diagrams/generate_diagrams.py
```

- `core-architecture.svg` — clients, authoritative server/sim, bridge,
  PDS edge, and the trust boundary.
- `gameplay-loop.svg` — local turn loop, job-board accept subloop, and the
  asynchronous ATProto edge.

Both are embedded in `docs/ARCHITECTURE-PROPOSED.md`.
