# 06 — Characters & Progression

> Original had ~40 stats/skills, body/mind split, jobs from janitor to assassin. Kept, but tiered.

## Structure

```rust
struct Body { hp, pv, dv, str, dex, spd, tough, appearance, hunger, thirst, sleepy, vision, hearing, battery? }
struct Mind { learn, mindpower, conscience, charisma, alertness, skills: Map<SkillId,u8>, xp: Map<SkillId,u32> }
struct Being { body: Body, mind: Mind, hands: u8, wear: WearSlots, impls: Vec<Bionic> }
```

- `PC = Being + PlayerMind` (any species can be possessed; wrapper, not special class).
- `ProgramPC`: full Mind uploaded via helmet; body stays, mind token enters Deck/Veil with `hp_mind`. Death → eject + mindpower drain, not instant permadeath (hardcore optional).
- Types: `human | alien | robot | animal | immobile_agent`.
  - Robots: 3-4 hands, no eat (charge/oil), weak to water/elec, built-in tools, battery.
  - Animals: no skills, mouth/hands carry tiers, special attacks.
  - Immobile agents (alarms, turrets, buttons): observer + tiny mind, no skills. `Handle` to direct nearby NPC to press.

## Stats (0-20 soft, 100 hard; HP/MP uncapped)

Core kept with modern curves:

- **STR:** `burden = ST*100g*tech_mult`. >120% slowed, >140% damaged, >150% pinned. Throw: `1 + (ST-10)/2` cells bonus; `dmg_bonus = ST/10`. Train burdened-walk/gym.
- **DEX:** drop/handling + pickup time. `cp = max*DX/50`; fail curve linear; `DX>50` gives action-time cuts.
- **SPD:** seconds per tile; `SPD>100` = % zero-time + bonus step. Trains only on track/gym + footwear.
- **Vision:** `radius²` cells; sparse FOV LUT (0:1,1:5,5:21,10:37,25:81,50:161,100:317). Carrots joke retained as +1 meme item.
- **Hearing/Eavesdrop:** `p = loud*hearing*4/dist`, walls block, global 2-queue sound bus. One conversation at a time by priority.
- **Hunger/Thirst/Sleepy 0-100:** staged debuffs (hungry/starving/faint; dehydrated/confused; tired/hallucinate/death at 100). Rates: hunger +1/30min active, thirst 2×, sleep 18h human / 16h alien / 21h robot-uptime.
- **PV (evade) / DV (soak):** PV checked per incoming blow (Dodge raises rate to 20/s), DV subtracts. Clothes modify per-part (§07). PV trains by being missed, DV passive.
- **Mindpower:** VR fuel + resist. `<5` kicked from Veil, `0` = erasure (hardcore death). `5` to enter.
- **Conscience 0-100:** warns before illegal act, scaled; abstain +XP, offend −XP, harder to move at extremes.
- **Charisma:** listen-priority + price/mission acceptance. Trains by gifts/advice.
- **Learning:** XP multiplier + memory retention; 20 = 100% retention.

Intrinsics: counted set (`add/remove/has`), incl. `lucky/unlucky` (global dice nudge, PC-only), resists/immunities (heat/cold/acid/elec/poison/sleep/confuse/paralyze), states (deaf/blind/hallu/paralyzed/asleep/poisoned/drunk/invis), senses (`see_invis, xray, infravision`).

## Skills (gated, XP on success)

Melee: `swords, clubs, whips` (+hit/+dmg/−time) • Ranged: `bows, guns, launchers` (+hit/−time only) + `recoil_avoid` (chance, not dmg) + cross-XP (bow→gun small, gun→launcher small) • Active melee: `high_kick (bypass DV), spin_kick (AoE), yank (swap), backflip_kick, fake_punch (bypass dodge), double_throw, shake (disarm), jump_attack, double_aim` (<20 self-injury risk) • Utility: `alertness, listening, lockpick, pickpocket, traps, tech, stealth, chem, athletics, cooking, dodge, garden, preserve, swim, climb, jump, dual_wield, courage, first_aid, healing, literacy, comp_literacy, music, driving, boating, skating, mechanic, electronics, electrobio (cyborg surgery, req high mech+elec)`.

- `comp_literacy` gates Deck mod tiers; `mindpower` gates Veil stunts + `mental_transfer` (take over lower-mindpower host nearby).
- No player-allocated stats at creation. Age sets base, class/type adjusts, level dominates. Level ~ rank within class + item/stat height, not raw power (L50 ≈ +5 over L1, not ×10).

## Jobs (data, not classes)

`police/ranger/defence` (ranked gear + arrest + jurisdiction) •
`cook/mechanic/electrician/builder/driver/storekeeper/tech/electrobiologist/doctor/weaponsmith/janitor/librarian/jeweler/guard/deliverer` (earn on task/sale, spend on stock, strike if unpaid) •
`researcher` (lab + discovery pay) •
`tourist/camper/fisher/hunter/shopper/gambler/partier/racer/scripthacker/vr_junkie/fighter` •
criminal: `burglar/thief/assassin/dealer/forger/kidnapper/spy/hired_hacker/vandal` (kept but with consent-gated targets + non-graphic abstraction).
Membership (guild/clan) gates bonuses/access/knowledge — XP alone insufficient for top tiers.
