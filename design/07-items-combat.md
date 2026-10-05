# 07 — Items & Combat

## Item model

```rust
struct Item {
  name, glyph, weight_g, hp, max_hp: Option<u32>, // None = indestructible terrain-like
  dv: i16, fragility: f32,
  slots: WearSlot?, hands_req: u8,
  on: HashMap<Hook, Effect>, // wield/wear/eat/drink/read/hit/pickup/drop/attacked/element
}
enum Hook { OnWield, OnWear, OnEat, OnRead, OnHit, OnElement(Element), OnUse, ... }
```

- Hooks replace original “items may do things when …” list, data-driven.
- Liquids as items with `volume_ml + flags {quenches, rusts, shorts, fuels, ...}` + element triggers. Pour/dip/spill distinct; mixing by proportion.
- Plants grow/spread, chopped → wood; watering/fertilizer events.
- Batteries: charge items, often contained. Containment by EntityId, not nested copies — fixes original holder/GC pain.
- Carry: `STR`-based + hands (2 base, robots 3-4, bionics add). Belt auto-holds weapons on tool-use (1t swap each way — deliberate vulnerability window). Pockets light-only. Worn-container closed while worn.

## Combat — Street

d20 + skill vs PV, then DV soak. Deterministic seed, luck nudges.

```text
to_hit = d20 + atk_bonus(skill + dex + weapon_acc + aim) vs 10 + target_pv + range_falloff + cover
  crit on 20 (×2 dice), fumble on 1 (glitch/self-check)
dmg = weapon_dice + str_bonus(ST/10 melee only) + mod_bonus
soak = DV_covering_part + shield_skill_reduction
hp_loss = max(0, dmg - soak)
recoil: if gun/launcher and fail recoil_avoid → self chip (bypass shield, reduced by resist)
```

- Ranged allows any angle; path = supercover line, no skipped cells; PV decides graze vs pass-through; shooter liable for friendlies.
- `Catch`: if PV stops projectile and hand free + weight ok → catch.
- Melee/ranged each get one target/turn base; `dual_wield` adds `MHC/100` extra + fractional chance.
- Kicking slides target + footwear dmg; only kick or `force` opens locked doors (loud).
- Throwing stepwise + terrain contact (replaces instantaneous line).

### Body-part option (default ON for PCs, OFF for crowds)

18 + 20 fingers (head/ears/eyes/mouth/neck/back/chest/waist/arms/hands/fingers/legs/feet). Head low chance, extremities favored on miss. Head/chest/back/leg/arm at 0 = kill; hands/feet/eyes/ears/mouth/fingers = maim. Armor covers parts (shirt=chest+back, helmet=head+contents). Overkill discarded. UI: `HP + head HP` summary; full sheet on demand / with `diagnosis` skill.

Elemental (heat/freeze/wet/acid/elec/radiation) separate from kinetic; same hook path with different wounds (burn/freeze/rust/erase/scramble). Elec erases magnetic disks — keep, it’s fun.

## Combat — Deck (tuning targets)

- TTK: equal-tier `Killer` vs `Killer` = 3-4 hits. `Shield` adds +2 TTK. `Takeover` = 2 acts + opposed `mindpower/comp_literacy` check.
- Costs keep spam down: `strike 1 act, takeover 2, clone 2 + cpu, crack N turns`.
- Balance invariant: `cpu_per_seg × len` must exceed `dps` at some len, so turtling giants starve themselves.
- Antivirus vs virus: detection = `sig_freshness × scanner_level vs virus_stealth`; stale sigs fail loudly.

## Loot / generation

- By `level + district`: `quality = level + district_mod + d6`. Extremes rare (~5 common on 0-10 scale per original note).
- Guarantees bypass RNG but still pass reachability check (§02).
- No vendor trash flood: most drops are `materials/charge/meds/sigdb`, not +1 swords.
