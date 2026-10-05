# 02 — Physical World: Street + Interiors

## Requirements (from original, kept)

- Seamless outdoor sections, different size/shape interiors, doors as portals.
- Types: residential / industrial / gov / military / rural / forest / lake / shops / transit.
- Roads/water continuity, blocked edges, difficulty by distance + type.
- Power per section affects network routing.

## Modern model

### Coordinates

```text
CityId / X,Y (section, i32, bounded ±24) / Z (level: -3..8) / x,y (tile u8 0..15)
EntityId = u64 (city_shard(8) | kind(8) | counter(48))
TileId   = CityId + XYZ + xy  // computable, no GUID string parsing
```

No more `Sect,-4,32` string splitting on hot path. Human-readable form
only for debug (`city1:S(-4,32):F1:T(3,9)`).

### Streaming (replaces 9 vs 13 vs 9+16 halo debate)

- **Active:** 3×3 sections around player (9 × 256 = 2304 tiles) ticked.
- **Visible:** + halo ring (total 5×5 = 25) loaded but frozen, for LOS/edge moves.
- **Interiors:** only current floor + directly connected floors active.
  Connected hosts’ Deck state lazy-loads on `connect`, not on proximity.
- Budget: tile ~64B + sparse entity lists → active set < 5 MB. Original “1KB/cell → 6MB” fear is gone with SoA + sparse storage.

```
 ticked:  ###      loaded-frozen:  &&&&&
          ###      + halo          &###&
          ###                      &###&
                                   &###&
                                   &&&&&
```

Crossing boundary: swap row/column, handoff entities, no teleport pop.
Prediction: preload in move direction.

### Generation

Two-phase, seeded, reproducible:

1. **City plan (offline):** district painter with constraints:
   `difficulty = f(radial_distance, district_type, story_stage)`.
   Roads via L-system with lane/dir metadata; rivers via random walk
   with edge-port preservation (`north_exit_x` must equal neighbour `south_entry_x`).
   Blocked edges via explicit `EdgeBlock` tiles, not missing data.
2. **Section instantiate (online):** fill from plan + Poisson clutter +
   guaranteed placements (quest items, computers, doors). Corridors/rooms via
   `empty-divide vs solid-carve` both supported; overlap allowed except over doors.

- Interiors stored as **Floor** (one file/blob per floor, rooms as subrects).
  Pre-authored POIs override generator rects.
- Deadlock check: BFS from each spawn/door to each guarantee; if unreachable,
  carve utility door or fail generation loudly (test catches, not runtime hack).

```yaml
section_def:
  district: residential_mid
  difficulty: 3
  roads: [{axis: EW, y: 7, lanes: 2, speed: 30}]
  guarantees:
    - {kind: computer, archetype: home_pc, at: room:kitchen}
    - {kind: egress, dir: E, to: S(-3,32)}
```

### Key tiles / props

- `barrier`: wall/fence (HP, destroyable). `portal`: door/stair/hole/gate
  with `target: TileId + spawn_offset + key_req`.
- `ground`: dirt/concrete/road/water/ice/lava + `moveCost × speed` + liquid morphs.
- `computer_anchor`: physical PC with `hostId`. Laptops/palms are items + hosts.
- `power_node`: section power; `powerOff` forces Net re-route (§04).
- Transport: boards (carry inside, etiquette risk), bikes (auto-park),
  cars (road-only, crash + police attention). All are `Vehicle` with
  shared-HP model (§07).

### Time

Energy clock, not round-robin “your computer moves first”:

- 1 tick = 1s game time. Entity has `nextActAt`; scheduler is priority queue.
- Speed 10 ≈ 10s/tile walk; car ~1s/tile; throw 1s flat.
- Sleep/hunger/thirst accumulate in active seconds only; frozen chunks
  apply catch-up decay on load (no “burning barrel forever” bug).
