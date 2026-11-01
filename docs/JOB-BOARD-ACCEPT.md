# Job Board “Accept” — Design Note

> Status: exploration, not scheduled. ATProto contract/reputation/bounty
> intents are deliberately parked; this note only asks what an **accept**
> step enables inside the current sim.

## 1. Current baseline

Today `Verb::Work(index)` is an **infinite, instant, local** action:

- pick one of five static jobs in `data/jobs.yaml`,
- consume loop/data/algo components,
- spend mood/food,
- get paid immediately.

There is no offer, employer, reservation, completion condition, deadline,
failure, or reputation. The “board” is a row of buttons.

**Adding `Accept` before the same `Work` action enables almost nothing.**
The value comes from what acceptance *commits* the player to.

## 2. What “accept” changes

| Without accept | With accept |
|---|---|
| Infinite repeatable job list | Finite offers that refresh/expire |
| No choice beyond pay/requirements | Choose by pay, risk, deadline, employer, location |
| Work is instant | Contract has a state and a completion condition |
| No consequence for stopping | Abandon/expire/fail has a cost |
| No relationship | Employer/clan reputation and follow-ups |
| No reason to leave the board | Contract can target a host, NPC, tile, or route |
| ATProto is just mail | Board offers can arrive from other cities/players |

## 3. What it enables

### 3.1 Agency and scarcity

The board offers a small set of competing contracts. Accepting one is a
planning decision:

- “240c to patch a driver before 18:00, but it needs 6 data components.”
- “500c to hack a Betanet host, but it files a crime and needs a license.”
- “80c courier run across the river; safe but eats the afternoon.”

Scarcity makes pay/risk/requirements meaningful; infinite Work does not.

### 3.2 Directed play across all four views

A contract can name a target instead of an abstract button:

- **Street:** deliver a package to an NPC/tile before a deadline.
- **Net:** route to a specific host without crossing a dark section.
- **Deck:** breach a host, retrieve a file, or kill a defender.
- **Veil:** carry a mind-file out of a hazardous area.

This is the biggest win: the board becomes a **quest generator** that
points the existing systems at each other.

### 3.3 Risk and consequences

Acceptance can carry risk terms:

- illegal contracts file crimes and draw cops;
- combat/bounty contracts can down the player;
- a failed deadline damages reputation or forfeits a deposit;
- a legal contract may require a clean record or a license.

That turns the existing crime/heat/license systems into **job selection
pressure**, not just post-hoc punishment.

### 3.4 Economy depth without inflation

Acceptance enables escrow and deposits:

- employer’s account holds the reward at accept time;
- player may post a deposit for gear/access;
- payment is released on verified completion;
- failure pays the employer/forfeits the deposit;
- offers are finite, so income is bounded by board refresh + deadlines.

This is materially different from `Work` minting cash on every click.

### 3.5 Reputation and progression

Every completed/failed contract is a record:

- employer trust gates better offers;
- clan contracts require membership/reputation;
- illegal work raises heat and closes honest offers;
- clean work + license opens corporate/gov contracts;
- repeated failure means lower pay or no offers.

This gives the player a **career trajectory** without a class system.

### 3.6 Social and federated play (later)

Once the ATProto bridge exists, the same accept mechanic can consume
remote offers:

- cross-city job posts on `sh.fidom.board`;
- player-posted contracts with escrow;
- clan hiring notices;
- asynchronous multiplayer: post a job, someone in another city takes it.

But the board record is **discovery only**. Acceptance must be an
authoritative local/server action; a public PDS cannot provide exclusive
claims or escrow.

### 3.7 Content generation

Contract templates can be data-driven by district, role, difficulty, and
risk:

- `deliver(components, npc, deadline, pay)`
- `hack(host_arch, file, deadline, pay, illegal)`
- `courier(tile, npc, deadline, pay)`
- `bounty(npc_role, deadline, pay, illegal)`

One data file can generate many offers without hand-authoring quests.

### 3.8 Pacing and anti-grind

Deadlines compete with food/mood/rent/sleep. A player cannot accept every
job, and cannot grind one infinite button. The board creates **session
goals** and natural stopping points.

## 4. What it does *not* enable by itself

- **No new verbs.** Contract completion still uses Work/Deck/Net/Street/
  Veil; accept is a coordinator, not a second game.
- **No ATProto authority.** A board record cannot reserve an offer,
  guarantee payment, or prove completion.
- **No depth from a static menu.** Without deadlines, requirements, risk,
  and consequences, “accept” is just “Work” with extra clicks.
- **No cross-city money by itself.** Payment needs a trusted ledger/escrow
  authority, not a record on a PDS.
- **No player-posted contracts yet.** Those need identity, escrow,
  disputes, and anti-abuse.

## 5. Minimal viable mechanic (recommended first slice)

Start local; no ATProto dependency.

### State machine

```text
posted -> accepted -> in_progress -> completed
                    \-> abandoned
                    \-> expired
                    \-> failed
```

### Data

```rust
struct JobOffer {
    id: String,
    employer: EntityId,       // local NPC/account for MVP
    title: String,
    kind: OfferKind,          // Deliver | Hack | Courier | Bounty
    payout_cents: i64,
    deposit_cents: i64,       // optional
    requirements: Requirements,
    deadline_tick: u64,
    risk: Risk,               // legal/illegal, combat
    source: OfferSource,      // Local | AtUri (later)
}

struct Contract {
    offer_id: String,
    state: ContractState,
    accepted_tick: u64,
    deadline_tick: u64,
    progress: u32,
    escrow_account: u64,
}
```

### MVP offer types

1. **Component delivery** — “bring 6 data to the shopkeeper by 18:00.”
   Reuses the existing component/stock loop; completion is a Deliver verb
   next to the employer.
2. **Hack target** — “breach host 1’s office-ws before the trace burns.”
   Reuses Net route + Deck exploit; completion is a flag set by the Deck
   act, then Deliver.
3. **Courier** — “take this package to tile X / NPC Y.”
   Reuses Street movement and portals; completion on arrival and Deliver.

Start with **one** type (component delivery) to prove the state machine,
then add hack/courier.

### Rules for the first slice

- One active contract at a time (or a small cap).
- Board refreshes once per game day; offers expire.
- Accept may require a deposit; abort forfeits it.
- Payout comes from the employer’s ledger account (escrow), not a mint.
- Failure/expiry lowers employer reputation; completion raises it.
- Illegal contracts file crimes through the existing `report_crime` path.
- `advise()` and the HUD show the active contract and time remaining.
- Save/load and replay include contract state; old saves default to empty.

## 6. Where ATProto fits, when we get there

- `sh.fidom.board` posts are **advertisements** for offers.
- The bridge imports/validates an offer into the local `JobOffer` table.
- `Accept` is still a local `Intent`; the server records the claim.
- Cross-city completion needs a local escrow/verifier or a trusted city
  server; a PDS record alone is not proof.
- Player-posted contracts are a later state: identity + escrow + dispute
  handling + anti-spam.

## 7. MVP acceptance criteria

- A data-driven offer appears on the board with pay/requirements/deadline.
- `Accept` creates a contract and reserves the offer.
- The contract can be completed through the existing action path.
- Payout moves from escrow, not thin air.
- Abandon/expire fails cleanly with the stated penalty.
- Save/load roundtrip and replay hash remain deterministic.
- Headless GUI test: board -> accept -> complete -> paid.
- No ATProto required for this slice.

## 8. Open questions

1. One active contract, or a small cap with deadlines?
2. Are deadlines in ticks or game days? How visible is the clock?
3. Does accepting require a deposit, or only failure penalty?
4. Can a contract be abandoned without penalty once, or never?
5. Are offers unique per city/player, or repeatable templates?
6. Do NPC employers need schedules/locations, or is the board abstract?
7. Which existing verbs can complete a contract, and who verifies it?
8. Does reputation live on the record, the employer, or both?
9. When does the board go ATProto-first versus local-first?
10. How do clan/company contracts differ from individual ones?

## 9. Recommendation

Build the **local component-delivery contract** first:

- it reuses `Work`, stock, shops, ledger, and the existing deadline clock;
- it proves accept -> active -> deliver -> escrow payout -> fail/expire;
- it gives the board real gameplay without waiting for the PDS bridge;
- once stable, add hack/courier types, then mirror offers through
  `sh.fidom.board`.

The key design test: if removing the Accept button would not change what
the player does, the mechanic is not adding gameplay yet.
