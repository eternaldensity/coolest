# 04 — Network, Identity, and Services

## Net map (was: network connection map; topology from 2006)

- Directed graph, fog-of-war. Node shows on first `scan/connect/sight`.
- Node card: `type, ip, name?, owner?, sec_level, cpu/ram/disk, ports free/used, bw`.
- Edge: `current/supported traffic → latency`. Red = saturated, grey = down/seen-before.
- Multi-path node renders once with badge `×n`, not duplicated (fixes original “show multiple times” clutter).
- Line length = importance (short = LAN, long = WAN/microwave).
- **Oldnet:** wired links (`wire list` per modem, damageable, repairable via `NetworkTester`),
  decaying, illegal/personal sites. **Betanet:** wireless (range-gated, encrypted) +
  laser backbone (fast, undamageable, toll per hop), gov/commercial only.
  Cross-net needs `Gateway` HW+SW + bounce. Backbone tolls + wireless intercept
  (mobile + headgear while walking) make route choice economic, not just short-path.
- Firewalls filter by `connection filter` (addr/file/behavior rules); can be taken over, not just DPS’d.
- Power: killing a section’s `power_node` forces re-route; microwave towers jammable for selective spy/jam if you hold the node.
- Transfer cap = `min(modems along path)` (both ends + intermediaries). Your own box hackable — same rules both ways.

```text
IP: major.local.device  e.g. 10.4.77  (Café Infinity = 10.x → 65k hosts)
Route cost = Σ(latency/traffic) + firewall_checks + protocol_xlate
```

### Discovery vs anchoring

- Every `HostId` maps to either `TileId` (physical anchor) or `Laptop(itemId)`.
- Net can reference ungenerated hosts via `Reservation { ip, archetype, anchor_hint }`.
  First `connect` or physical visit materializes it. No “can’t hack ungenerated room” dead end.
- Laptops move: anchor follows holder; route recomputes; existing chains persist until break.

### Cable / wireless (was: Use cable + letter + direction)

- Adjacent or same-tile: `Use cable → pick target` (modern picker, not letter memorization).
- Held-to-held cable persists while held; break on distance >1.
- Radio: `radio_on/off`; in-range auto-link with toast. Only active links render (no stale disabled edges).
- All links appear in `KnownLinks` panel with signal + break warning.

## Identity / accounts + devices/HW (2006)

- Handles immutable; display names mutable. Guest sandbox auto-creates `guests/<handle>` on first login.
- Passwords: per-account salted hash (simulated: `crack_difficulty`). Multi-turn skill-gated crack with progress + interrupt (2006 rejects letter-at-a-time as silly). Once cracked, password shared to all your programs until rotated.
- `admin_password` capture ≠ admin control. Grants: enter Private without aggro + run in Private. Remote admin control requires entry in `remote_admin_allowlist` (heavily guarded, hidden).
- Local + admin login at keyboard grants direct admin-program control. Remote never does — must flip allowlist first.
- Devices: `name, HW list, boot order`. HW: `cpu (speed), memory (slot count), drives (programs/data), modems (wired=list+damageable / wireless=range / laser=fast+untouchable)`. Ops: `turn on/off, add/remove HW/drive, destroy`. HW swap costs install time (pay to rush). Types: `workstation, laptop (carryable), PDA (small, often unlocked), server (multi-user, no direct use), node/exchange/repeater (connect only + logs), firewall (filters), supercomputer`. Your starter = cheapest PC + net + firewall (2024 start).
- Files (data-driven): `driver, user DB, log (connections *through* device), startup (drivers + login msg), connection/email/behaviour filters, patch, memberlist, customer/catalogue, criminal record, bank, travel/passport, inbox/outbox/addressbook, tips, Betanet map, ops plans, vuln warnings`.

## Email / chat (in-sim + ATProto bridge)

- From `Mailer` on `Access*` only. Inbox/outbox capped (e.g. 20). No drafts/trash (kept as flavor limit).
- Attachments are whole files (all sectors together). Must save to disk to run.
- Viruses can auto-run on open depending on `Mailer` hardening. Logged `sent_at/received_at`.
- Mailbox hack requires hack-enabled `Mailer+` (rare loot/craft).
- Federation: `sh.fidom.mail`, `sh.fidom.board`, `sh.fidom.profile` lexicons on player PDS. Cross-city post/email syncs via Relay/AppView; in-sim delivery still passes through `Mailer` trace/crack checks. Positional `say` stays sim-only.

## Banking / economy (was: thumb pay, ATMs, “can’t magic numbers”)

Double-entry ledger, no duping:

```rust
struct Account { id, owner, branch_host, balance_cents: i64 }
enum Tx { Transfer{from,to,amt,memo}, Mint{to,amt,reason}, Burn{from,amt,reason} }
// invariants: Σ balances == Σ mints - Σ burns; every transfer has source
```

- Thumb tap = instant transfer if both online; else queued.
- Cash exists for offline resilience. ATMs dispense against ledger; if branch host down, ATM serves from cached cap + marks `IOU` to reconcile.
- Bank hosts: sharded by branch, gateway + HSM-like `ledger_core` (heavily guarded, layered private nets).
- Fake banking UI is a real attack: clone `BankUI` mod, point victims at it, harvest creds — but settlement still hits real ledger, so you must move money, not edit numbers. Decompiler/compiler enables crafting it (§03).
- Sinks/sources tuned: jobs/shop sales = mint, rent/food/transport = burn, crime = transfer.

## Mirrored authoritative data (was: mirroring)

Criminal records, licenses, balances, gun permits live in `LedgerService`,
not in any one host. Hosts hold **replicas/views**:

- Crashing a replica never destroys truth; only degrades access.
- Editing requires quorum + difficulty (e.g. alter record on 2 of 3 replicas + beat `audit_daemon`).
- Read-only shared areas enforced by capability, not politeness.
