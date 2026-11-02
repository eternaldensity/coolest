# New Player Guide — FIDOM: Future Internet Domains of Mayhem

> Status: sim + headless client playable via `cargo run -p client-web`;
> desktop shell via `cargo run -p gui`; web shell via `cd gui && trunk serve`.
> All four views have server-routed acts and headless UI tests
> (accesskit, no screenshots). v0.4 added HP, gear, street combat, cop
> arrests, and a real-mindpower Veil.

## The pitch

Live in a town. Break into its computers. Become what you do.
Two networks span the city: decaying wired **Oldnet** (free, illegal) and
regulated wireless **Betanet** (commercial, watched, toll roads). You start
broke with the cheapest rig and eat, work, and hack your way up.

## Your first minutes (the survival loop)

You wake with **food 80 / mood 70 / $2.00 / 3 rations**. Every action ticks
both bars down:

- **Work** a programming job → earns cash, costs mood + food. Five tiers
  on the board (`debug-script` 60c → `contract-build` 500c): higher tiers
  pay more per mood but drain more pressure per act.
- **Jobs consume components.** Every job needs 4–6 units of `loop`,
  `data`, or `algo` from your rig's stock; the button shows
  `[resource have/need]`. Two ways to get them:
  - **Code** (Street tab / `code loop|data|algo`) — manual: 1 unit per
    act, plus a small mood cost. Slow.
  - **Farm** (Deck tab, on a selected cell) — deploy a producer that
    yields 3/2/2 units per act, free, but costs rig CPU (2/3/5).
    Higher tiers are hungrier, so mid/high jobs want producers.
    Producers are not intruders, so the range warden ignores them.
- **Eat** a ration → +30 food. No rations? Order more (online stores, M1).
- **Watch** a show → +15 mood, costs $0.30.
- **Sleep** in your rented room → rests to 06:00, no needs cost beyond
  the act itself. Away from home (or with no room) it is refused.
- At **zero food or mood, jobs lock out**; starving doubles how long
  everything takes. This is the whole early game: stay fed and cheerful
  enough to keep earning.

## Shops, hours, and the night store

- Counters serve adjacent tiles (`$`, gold). Day shop (8:00–20:00):
  ration 100c, meds 400c, firewall 1500c, knife 300c, jacket 500c,
  medkit 250c. Shuttered counters refuse with the reason shown — come
  back in the morning.
- Night store three doors east stays open 24h at steeper prices
  (ration 150c, meds 600c) for the graveyard shift.
- Talk to the shopkeeper: `Trade` routes to the counter, no haggling.

## Contracts and the job board

The Street tab now has a daily **contract board** in addition to the
always-available Work buttons.

- Offers show title, pay, requirements, risk, and time left. `Accept`
  reserves one offer and escrows the employer’s reward.
- You can hold **one active contract**. Complete it with
  `Deliver contract`; give up with `Abandon contract` (the deposit is
  forfeited and employer reputation drops).
- Delivery contracts consume medkits/snacks or loop/data/algo components.
  Courier contracts require standing on the target tile. Hack contracts
  require the target host to be breached. Bounty contracts require the
  target to be gone.
- Escrow is real ledger money: completing pays the employer’s reward plus
  your deposit back; failing/expiring returns the reward to the employer
  and costs reputation.
- Illegal contracts file crimes when completed; legal employers may
  refuse you after too many failures.
- The board refreshes once per game day.

## Health, gear, and danger

- You have **HP** (20), **PV** (evade) and **DV** (soak). The status panel
  shows HP and your body stats.
- Buy gear at a counter, then `Equip` it from the Pack row. Knife/pipe are
  melee; the pistol is ranged; jacket/vest add DV; medkit/snack are
  consumables. Carry cap is `STR × 100g`.
- Adjacent NPCs show `Attack <role>` buttons. Hits roll d20 + DEX/4 + 1
  against `10 + PV`, then subtract DV. A natural 20 crits, a natural 1
  misses. Kills drop loot and file a crime; survivors fight back.
- Cops attack **Wanted** players on sight. Stand next to a cop and use
  `Surrender`: pay the fine, or serve the night in jail (cash seized,
  record cleared) if you cannot.
- At 0 HP you are **down**, not deleted: `Revive at clinic` restores half
  HP at home for a pocket-first fee, with any shortfall as debt.

## Deck, Net, and Veil acts

- **Deck:** click a cell, then choose a program kind and `Load` it.
  Killer/Shield/Scanner/Cracker/Connector/Cloner/Editor have different
  CPU, length, and role abilities. `Strike` heads/tails; the warden
  defends host 1, intrusions draw a trace, and kills count toward your
  record.
  - `Scan` (Scanner) reveals the host.
  - `Crack lock` (Cracker) takes three beats to drop the filter/monitor.
  - `Patch <vuln>` (Editor) closes a hole.
  - `Overwrite` (Editor) severs a tail segment without a headshot.
  - `Clone selected` (Cloner) copies a program to a free cell.
  - `Connect host 3` (Connector) probes a routed host, reveals its scan
    report, and logs the transit.
  - After a scan, the host line shows the scan report (vulns, logs, trace,
    breach).
  - Kind-matched `Synergy farm` producers yield +1 of their affinity
    resource.
  The rig is an `office-ws` with its own vulns: `Exploit` one (needs a
  program of yours there) to breach it. `Scrub log` erases your trail —
  a program on the host, or a privilege vuln, gets you in.
- **Mail:** the Street tab has a Mail row. `Craft Mailer+ (3 algo)` unlocks
  trojans; `Send trojan` mails one (the Deck control targets the rig's
  owner, or `Trojan <role>` an adjacent NPC). A trojan is an **attachment**
  — nothing happens until the recipient opens it, and NPCs read their mail
  on their own turn. When a trojaned mail is opened, the reader's hosts
  join your zombie net and pay capped daily spam income (400/host/day).
  Opening one yourself does the same to *your* rigs.
- **Net:** the tab lists hosts (net kind, modem, gateway, `DARK`) and links.
  `Route 1→3` previews path/tolls and reroutes around dark sections and
  firewalled transit; `Damage wire 1-2`, `Cut/Restore power`, and the
  `Block/Unblock` firewall filter are all logged acts.
- **Veil:** `Jack in` with 5+ mindpower, walk exit links, seize the
  area's bound host or `Transfer` into a weaker nearby NPC, carry/read
  mind-files, `Eject` (light drain). Hazard areas damage the mind; death
  ejects with heavy drain. A power cut ejects everyone and drains them.

## The four views (Tab to switch)

| View | What | Verbs wired today |
|------|------|-------------------|
| Street | Tile streets, doors, shops | move / eat / work / contracts (accept/deliver/abandon) / watch / sleep / buy / talk / attack / pickup / drop / equip / use / surrender / revive / deposit / withdraw |
| Net | Host graph, latency, tolls | route (preview) / damage wire / cut+restore power / firewall filter |
| Deck | Inside world host 1: RAM arena | load kinds / strike / scan / crack / patch / overwrite / clone / farm (kind synergy) / play cards / exploit + trojan / scrub log (warden answers) |
| Veil | Helmet space over the same rules | jack in / move / seize / transfer / carry+read files / eject |

The Deck tab also plays the **closed card table** (design/03): 8 negative,
8 positive, 4 manipulation cards. Attacks roll a d6 (a 5-6 crits for +1);
next-turn effects include shield, skip, stability poison, roll-minus,
ban-die, decoy, downgrade, reflect, splash, extra-roll, and random-target.
Cards **cost components to play** (1-2 loop/data/algo; `spike`/`overload`
fund their own damage, `farm` is free), so the resource loop feeds them.
The `Mods` row builds a **persistent** ability into your program
(`tank`/`plating`/`reflect`/`splash`/`extra`) — it fires on every exchange,
where the same card played from `Cards` is a one-shot next-turn effect.
Card play is a logged act, so replays reproduce the same dice.

Aspirational verbs from the design (scan, connect, crack, patch, embody)
are not wired to the shell yet; see `design/09-ux-roadmap.md`.

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
- **Wired up today:** sabotage (damage wire / cut power), veil seize and
  trojan spamming all file crimes. The status panel shows
  standing/alignment/mass; `Pay fine` (100c per severity point, pocket
  first) settles the record, which ends the chase and returns you to
  honest. `Join clan 1` pays dues + initiation (killing a clanmate's
  program makes you its enemy for life), and `Apply license` grants the
  license to hack on 3 threat kills (warden kills) + a clean record —
  licensed entry to honest machines is legal until you hit a defender,
  which revokes it with a fresh charge.
- **License to hack** (3 threat kills + spotless record) lets you enter
  honest machines to fight intruders legally. Hit a defender and it is
  revoked, plus a fresh charge.

## Dying (or not)

- Street: HP damage, cop attacks, and clinic revival (v0.4). Loot drops
  where an NPC falls.
- Deck: losing a program just costs the program.
- Veil: your mind enters at 5+ mindpower; death **ejects** you with heavy
  mindpower drain instead of killing you. Hazards damage the mind, and a
  power blackout ejects everyone. Below 5 mindpower you cannot re-enter
  until it recovers.

## Controls today

```bash
cargo run -p gui            # desktop shell (eframe)
cd gui && trunk serve       # browser shell (wasm)
cargo run -p client-web     # scripted tour of all systems
cargo test --workspace      # prove the rules hold (259 test executions)
```

Keys: arrows or `hjkl` move; `Tab` switches Street/Deck/Net/Veil; the
console takes `eat | work 0-4 | watch | sleep | buy <item> | pickup/
drop/equip/use <item> | talk | revive | deposit/withdraw <c> | help`.
Panes: status + objective (left), tabs (top), console + log (bottom),
view (center). Attack/Surrender are buttons on the Street tab. Ctrl-K
palette and `>`/`<` stairs are not wired yet — see
`design/09-ux-roadmap.md`.
