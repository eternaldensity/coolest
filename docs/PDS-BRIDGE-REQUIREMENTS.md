# PDS Bridge — Requirements & OSS Selection Brief

> Status: requirements only, 2026-11-01. No PDS networking exists yet.
> `server/src/atproto.rs` validates three embedded lexicons; it has no
> identity resolution, auth, HTTP client, queue, or PDS transport.
> The nightly PDS-kill chaos stage is therefore skipped.

## 1. What “the PDS bridge” means here

The bridge is an **edge adapter** between the authoritative FIDOM sim and
one or more ATProto Personal Data Servers. It is **not** a PDS, relay, or
AppView. It:

- writes `sh.fidom.*` records to a PDS (mail/board/profile),
- consumes matching records from a PDS or relay,
- maps external DIDs/handles to stable sim identities,
- validates, rate-limits, deduplicates, and queues,
- injects inbound mail into the sim **only** as a logged `Intent`,
- never executes attachments or writes sim state directly.

The sim remains deterministic: no wall clock, network, filesystem, or
threads in `sim/`; all gameplay mutation goes through
`World::apply(&Intent)` (design/11 rule 1).

## 2. Deployment modes

### Mode A — FIDOM-operated PDS (recommended first)

A PDS runs as a sidecar or separate service next to the game server
(official PDS in a container, or a Rust PDS). Players get accounts on it;
the bridge uses a service account/app password or OAuth session.

- Lowest risk; no third-party OAuth consent flow.
- Still uses real ATProto repos, DIDs, lexicons, and sync.
- Satisfies “ATProto at edge only”; can later federate.

### Mode B — player-owned PDS (federated)

Each player uses their own PDS (self-hosted or hosted). The bridge becomes
an ATProto client/AppView:

- OAuth 2.1 + PKCE + DPoP against each player’s PDS.
- Identity is the player’s DID; handle is display-only.
- Ingest via relay/firehose or targeted `listRecords` polling.
- Requires moderation, blocking, and signature verification.

### Mode C — read-only indexer (optional)

Consume a relay/Jetstream for board/profile search only; no writes. Useful
for global character search without full federation.

## 3. Non-negotiable invariants

1. **No sim bypass.** Inbound records become a `Verb`/`Intent` and appear
   in the server log; replaying the log reproduces the mailbox state.
2. **No executable content from ATProto.** Attachments are blobs/files
   that must be saved and opened in-sim; nothing auto-runs. Trojans are
   never federated as executable payloads.
3. **DID is canonical; handle is mutable display data.** Never key a
   player, mailbox, or blocklist by handle alone.
4. **All external data is untrusted.** Validate lexicon, `$type`, size,
   CID/AT URI, timestamp, and sender status before it reaches the sim.
5. **The bridge never blocks the sim tick.** PDS/relay latency and
   outages are handled by queues, not by stalling `apply`.
6. **No private game data leaves the process.** Only explicit mail/board/
   profile fields are published; positions, inventory, host graphs, and
   private logs are not.

## 4. Functional requirements

### P0 — first playable bridge

| ID | Requirement |
|---|---|
| BR-1 | Resolve a handle to a DID and a DID to a DID document containing the account’s PDS endpoint and signing key. Cache with TTL and negative caching. |
| BR-2 | Create, read, list, and delete records through XRPC with `$type`, TID record keys, and AT URIs. |
| BR-3 | Validate every outbound and inbound record against the `sh.fidom.*` lexicon (required fields, length caps, datetime format, `$type`). |
| BR-4 | Write plain mail, board posts, and profile records; never write trojan flags or executable attachments. |
| BR-5 | Poll `com.atproto.repo.listRecords` per collection with a persisted cursor; deduplicate by AT URI + CID. |
| BR-6 | Map DID → stable synthetic `EntityId` and persist the mapping; handle changes do not change identity. |
| BR-7 | Inject inbound mail as a logged `Verb` (e.g. `ReceiveExternalMail`) after checks; respect the 20-mail inbox cap with a bounded pending queue. |
| BR-8 | Persist an outbound queue with idempotency keys, bounded retries, exponential backoff, and dead-letter handling. |
| BR-9 | Survive PDS/network outages without sim failure: queue outbound, resume inbound from cursor, surface health. |
| BR-10 | A local smoke test against a real PDS: create account, write mail, list it, ingest into a mailbox, replay the server log to the same hash. |

### P1 — federation and public network

| ID | Requirement |
|---|---|
| BR-11 | OAuth 2.1 authorization-code + PKCE + DPoP; refresh, revoke, and never store player passwords. |
| BR-12 | Consume `com.atproto.sync.subscribeRepos` (CBOR/WebSocket) with cursor resumption, size limits, and per-DID ordering; or a verified equivalent (Jetstream/TAP) if record verification requirements are met. |
| BR-13 | Verify commit signatures and repo integrity when consuming an untrusted relay; reject invalid or future revisions. |
| BR-14 | Handle account status (`active`, `deactivated`, `suspended`, `takendown`, `deleted`) and stop writing/ingesting accordingly. |
| BR-15 | Blob upload/download for profile avatars and *non-executable* mail attachments; store blob refs, enforce MIME/size caps. |
| BR-16 | Report/moderation hooks: per-DID blocklists, report forwarding, per-sender daily caps, and community labels. |
| BR-17 | Backfill and resync via `getRepo`/`listReposByCollection` after cursor gaps; dedupe historical records. |
| BR-18 | Global board/profile index (SQLite/Postgres) for city job boards, clan notices, and character search. |

### P2 — later

- Account migration between PDS hosts; handle rotation.
- Multiple relays/PDS hosts and failover.
- Cross-city traveler identity with DID + city/counter continuity.
- Rich mail threads (`replyTo`, `root`, `parent`) and read receipts.
- Moderation UI and appeal workflow.

## 5. Protocol surface the bridge must speak

| Namespace / method | Use | Priority |
|---|---|---|
| `com.atproto.server.describeServer` | PDS discovery/health | P0 |
| `com.atproto.server.createSession` / `refreshSession` / `deleteSession` | service-account/app-password auth (Mode A) | P0 |
| `com.atproto.identity.resolveHandle` | handle → DID | P0 |
| DID resolution (`did:plc` / `did:web`) | DID doc, PDS endpoint, signing key | P0 |
| `com.atproto.repo.createRecord` | write mail/board/profile | P0 |
| `com.atproto.repo.getRecord` / `listRecords` / `deleteRecord` | read/sync/cleanup | P0 |
| `com.atproto.repo.describeRepo` | repo metadata | P0 |
| `com.atproto.repo.applyWrites` | batched writes (optional) | P1 |
| `com.atproto.repo.uploadBlob` / `sync.getBlob` | avatars/attachments | P1 |
| `com.atproto.sync.subscribeRepos` | firehose | P1 |
| `com.atproto.sync.getRepo` / `getLatestCommit` / `getRepoStatus` | resync/status | P1 |
| `com.atproto.sync.listReposByCollection` | backfill board/profile records | P1 |
| OAuth: PAR, authorize, token, revoke, JWKS | player-owned PDS | P1 |
| `com.atproto.label.*` / `com.atproto.moderation.*` | labels/reports | P2 |

Notes:

- XRPC paths are `/xrpc/<NSID>`; JSON at the boundary, DAG-CBOR inside
  repos. The PDS handles repo encoding; the bridge must not hand-roll MST
  or CAR unless it chooses to verify a firehose.
- Record writes need `repo` (DID/actor), `collection`, `rkey` (TID), and
  `record`; the PDS returns the AT URI and CID.
- `createdAt` is RFC3339/ISO-8601 with timezone. The current tests’ `"t"`
  values are placeholders, not protocol-valid.
- Never use handles as foreign keys in records; use DIDs. Keep a display
  handle as a separate field if needed.

## 6. Lexicon and record-model changes

Current lexicons are minimal and not yet protocol-complete. Required
changes before federation:

1. **`$type` validation.** Every record must carry
   `"$type": "sh.fidom.mail"` etc.; `validate_record` must require it and
   reject a mismatch.
2. **Datetime validation.** Enforce RFC3339 for `createdAt`/`expiresAt`,
   not merely “non-empty string”.
3. **Canonical recipients.** `sh.fidom.mail.to` should be a DID (or an
   explicit `toDid`), not only a handle. Add `toHandle` as display data.
4. **Threading.** Add `replyTo` (AT URI), `root`/`parent` if threads are
   in scope; otherwise state “flat mail only” and omit.
5. **Mail provenance.** `from` is the repo DID (implicit); add optional
   `subject`, `langs`, and non-executable `attachments` (blob refs).
6. **Board metadata.** Add `kind` (`job | clan | notice | event`),
   `expiresAt`, `contact` (DID), `tags`, `langs`; keep `city`.
7. **Profile.** `handle` is identity, not a record field; keep
   `displayName`, `description`, `avatar` (blob), `city`, `clan`,
   `updatedAt`. One record per DID with rkey `self`.
8. **NSID authority.** `sh.fidom.*` implies control of the `fidom.sh`
   domain for publishing lexicons; verify ownership or move to an owned
   authority before claiming federation.
9. **Versioning.** Breaking changes get a new collection/lexicon version
   and a migrator; never silently retag an old record.

Suggested record shapes (illustrative, not final):

```json
{
  "$type": "sh.fidom.mail",
  "toDid": "did:plc:...",
  "toHandle": "eve.example",
  "subject": "job",
  "body": "initiate here",
  "createdAt": "2026-11-01T10:00:00Z",
  "replyTo": "at://did:plc:.../sh.fidom.mail/3k...",
  "attachments": []
}
```

```json
{
  "$type": "sh.fidom.board",
  "city": "c1",
  "kind": "job",
  "title": "debug script",
  "body": "60c, loop components",
  "contact": "did:plc:...",
  "tags": ["job", "loop"],
  "createdAt": "2026-11-01T10:00:00Z",
  "expiresAt": "2026-11-08T10:00:00Z"
}
```

## 7. Sim-side changes required

- **External identity registry.** `DID → EntityId` persisted outside the
  sim (bridge DB) but the chosen `EntityId` is carried in the inbound
  `Intent`, so replay is deterministic.
- **Inbound verb.** Add `Verb::ReceiveExternalMail { from_did, from_handle,
  at_uri, body, created_at, tick }` (name TBD). It is accepted only from
  the privileged bridge path; the server logs it like any other intent.
- **Mail provenance.** Extend `Mail` with optional `at_uri`, `from_did`,
  `from_handle`, and an external flag. Existing saves default to local
  mail; never reorder fields.
- **Outbound trigger.** The server needs a deterministic, drainable
  `BridgeEvent` (or event payload) carrying the local sender/recipient
  `EntityId`, body, and mailbox id, emitted after a successful
  `SendMail`/`SendTrojan` act. The bridge resolves the recipient DID.
- **Never federate trojans.** `SendTrojan` stays local. Inbound mail with
  an attachment is quarantined until the player saves/opens it in-sim;
  opening applies the existing trace/crack rules.
- **Mailbox backpressure.** Inbox cap 20: keep a bounded pending queue
  (oldest-first, TTL) rather than dropping records silently; expose
  “mailbox full, N pending” to the player.
- **Board/profile are not gameplay state.** They can live in the bridge
  index and be rendered read-only; no need to mutate `World` for a board
  post. Mail is the only P0 inbound mutation.

## 8. Ingest and outbound pipeline

### Inbound (P0: polling; P1: firehose)

```text
PDS/relay -> validate envelope + lexicon -> dedupe (AT URI/CID)
          -> resolve/allocate DID->EntityId -> policy (rate/block/status)
          -> pending queue -> Verb::ReceiveExternalMail -> server log
          -> World::apply -> mailbox
```

Rules:

- Persist `(collection, cursor, last_rev)`; process per-DID in order.
- Ignore future `rev`/`createdAt` beyond a small clock-drift window.
- Reject records >1 MB, malformed CBOR/JSON, missing `$type`, or
  lexicon-invalid fields.
- Idempotency key: AT URI + CID. Re-delivery is a no-op.
- Unknown senders: allocate an external identity only after rate-limit and
  provenance checks; otherwise quarantine.
- If the mailbox is full, retain in the pending queue and retry; never
  write directly into `World`.

### Outbound (P0)

```text
SendMail/board/profile act -> BridgeEvent -> resolve recipient DID
 -> build record ($type, TID rkey, createdAt) -> validate
 -> enqueue(idempotency_key) -> XRPC createRecord -> store AT URI/CID
```

Rules:

- One local act produces at most one PDS record; retries reuse the same
  rkey/idempotency key.
- 429/5xx: exponential backoff with jitter; 401: refresh once, then
  dead-letter and surface an operator alert.
- PDS write success does not retroactively change the sim; the local
  mailbox already recorded the send.
- Board/profile writes are optional for P0; mail is the acceptance path.

## 9. Failure, idempotency, determinism

- **PDS down:** outbound queue grows within caps; inbound polling pauses;
  sim tick is unaffected. Health endpoint reports `degraded`.
- **Relay gap/firehose disconnect:** resume from persisted cursor; on a
  detected gap, mark the repo desynchronized and resync via `getRepo`.
- **Duplicate delivery:** dedupe by AT URI + CID; replay of the same
  inbound Intent must be safe or explicitly idempotent in the sim.
- **Clock:** use bridge wall clock only for `createdAt`/TID/backoff. The
  sim records the tick at delivery. Never let bridge time enter `apply`
  except as logged Intent data.
- **Chaos gate:** kill the PDS/relay mid-run; assert queue drains after
  restart, no panic, no duplicate mail, and the server replay hash is
  unchanged for the same Intent log.

## 10. Security and privacy

- HTTPS only for PDS/relay; validate certificates.
- SSRF protection for DID/handle/PDS URLs: allow `https`, block private/
  loopback/link-local addresses, cap redirects, re-resolve DNS safely.
- Verify DID-document keys and commit signatures when ingesting from an
  untrusted relay; do not trust a relay’s record contents alone.
- Store OAuth tokens/app passwords encrypted; never log them; support
  revocation and rotation.
- Rate-limit per DID, per IP, and per collection; cap body sizes and
  pending queues.
- Do not publish player position, host graph, inventory, private logs, or
  trojan payloads.
- Provide block/mute/report paths and honor PDS account takedowns.

## 11. Ops and configuration

Minimum config:

```text
FIDOM_PDS_URL=https://pds.example
FIDOM_PDS_DID=did:web:pds.example
FIDOM_PDS_HANDLE=...
FIDOM_ATPROTO_MODE=operated|player|indexer
FIDOM_ATPROTO_DB=...            # queue, cursor, DID map, index
FIDOM_ATPROTO_COLLECTIONS=sh.fidom.mail,sh.fidom.board,sh.fidom.profile
FIDOM_ATPROTO_POLL_SECS=30
FIDOM_OIDC_CLIENT_ID=...        # Mode B
FIDOM_OIDC_REDIRECT_URI=...
```

Operational needs:

- health/readiness for bridge, PDS, DB;
- metrics: queue depth, oldest job age, ingest lag, auth failures,
  dedupe hits, dead letters;
- structured logs with DID/AT URI but no secrets;
- backup/restore of the queue, cursor, DID map, and index;
- Docker compose for a local PDS + bridge + server.

## 12. Acceptance tests

P0:

1. Lexicon/`$type`/datetime validation rejects malformed records.
2. DID/handle fixtures resolve; handle change preserves DID identity.
3. Write/read/list/delete against a **real local PDS** (no HTTP mocks).
4. Duplicate/out-of-order/future records are handled per §8.
5. Inbound mail reaches the mailbox only via a logged Intent; replay
   reproduces the same hash.
6. Mailbox-full backpressure preserves pending mail.
7. Outbound 500/429/401 paths retry/refresh/dead-letter correctly.
8. PDS kill/restart: no sim panic, queue drains, no duplicate delivery.

P1 adds: OAuth + DPoP against a player PDS, firehose cursor resume,
commit-signature verification, account takedown handling, blob round-trip.

The nightly `PDS chaos` stage should replace the current “unavailable”
skip only when P0 (and ideally P1) acceptance is green.

## 13. OSS candidates

Use this as a shortlist, not a commitment. Verify activity, license, and
feature coverage at selection time.

| Project | Language | Role for FIDOM | Notes |
|---|---|---|---|
| [bluesky-social/atproto](https://github.com/bluesky-social/atproto) | TypeScript | Reference PDS, repo/MST/CAR, sync, identity, lexicon, XRPC, OAuth | Best correctness oracle; can run the official PDS. Dual MIT/Apache-2.0. |
| [bluesky-social/pds](https://github.com/bluesky-social/pds) | TypeScript/Docker | Official self-hosted PDS | Fastest Mode A deployment; operational docs. |
| [bluesky-social/indigo](https://github.com/bluesky-social/indigo) | Go | Relay/BGS, atproto libraries, firehose | Strong for relay/firehose; verify current PDS scope before relying on it. |
| [blacksky-algorithms/rsky](https://github.com/blacksky-algorithms/rsky) | Rust | PDS, relay, firehose, identity, lexicon, repo | Apache-2.0; closest Rust-native full stack; WIP, evaluate maturity. |
| [atrium-rs/atrium](https://github.com/atrium-rs/atrium) | Rust | XRPC client/API, bsky SDK | MIT; good outbound client/identity helper; not a PDS. |
| [musicjunkieg/atproto-crates](https://github.com/musicjunkieg/atproto-crates) | Rust | 20 crates: DASL/CAR, identity, repo/MST, lexicon, OAuth, client, Jetstream/TAP, PDS/entryway | MIT; broad and recent; pin a revision; evaluate API stability. |
| [dollspace-gay/Aurora-Locus](https://github.com/dollspace-gay/Aurora-Locus) | Rust | Self-hosted PDS (SQLite/Postgres), OAuth/DPoP, MST/CAR, firehose, identity, moderation | MIT/Apache-2.0; 893 commits; built on `proto-blue`. Small star count; verify conformance and ops before production. |
| [dollspace-gay/proto-blue](https://github.com/dollspace-gay/proto-blue) | Rust | SDK: syntax, crypto, lexicon, repo/MST/CAR, XRPC, WS, identity, API/Agent, OAuth | MIT/Apache-2.0; 14 crates, 368 generated types, interop parity tests. Explicitly AI-generated and pre-1.0; pin a revision and differential-test against the TS reference. |
| [dollspace-gay/skyauth](https://github.com/dollspace-gay/skyauth) | Rust | OAuth 2.1 client: DPoP, PAR, PKCE, SSRF-safe identity discovery | Fork of `mike10010100/skyauth`; dual MIT/Apache-2.0; v0.1, 18 commits. Overlaps `proto-blue-oauth`; use as an OAuth hardening reference or alternative, not a second stack. |
| [DrChat/bluepds](https://github.com/DrChat/bluepds) | Rust | Alternative PDS (Axum + Atrium, SQLite) | Apache-2.0; small project; useful reference or test PDS. |
| [DavidBuchanan314/millipds](https://github.com/DavidBuchanan314/millipds) | Python | Minimal PDS | MIT; explicitly experimental/testing-only; good for interop experiments. |
| [threddyrex/rustproto](https://github.com/threddyrex/rustproto) | Rust | PDS + repo/MST/CAR/firehose reference | MIT; small; useful as a readable implementation. |
| [desertthunder/tempest](https://github.com/desertthunder/tempest) | Elixir | Self-hosted PDS | Useful if the stack is Elixir; not a Rust fit. |

Also relevant: `bluesky-social/atproto-interop-tests` (conformance
vectors), Bluesky **Jetstream** (JSON firehose; easier ingestion, less
verification), and **TAP** (filtered/verified event delivery).

### Evaluation checklist

For each candidate, require:

- [ ] PDS repo CRUD (`create/get/list/delete`) and TID rkeys.
- [ ] DID/handle resolution and DID-document service endpoint.
- [ ] OAuth 2.1 + PKCE + DPoP (Mode B), or app-password sessions (Mode A).
- [ ] Firehose + cursor/backfill, or a documented polling fallback.
- [ ] Lexicon validation (at least permissive) and `$type` support.
- [ ] CAR/MST/repo verification if it will consume untrusted relays.
- [ ] Persistence: SQLite/Postgres, backups, migrations.
- [ ] Container/deploy story, health/metrics, logs.
- [ ] License compatible with FIDOM (MIT/Apache preferred).
- [ ] Active maintenance and a real test suite; prefer projects with
      interop tests against the reference implementation.

### Candidate notes (README + source-level spot checks; verify before selecting)

| Candidate | Use it for | Do not use it for |
|---|---|---|
| Aurora-Locus | A self-hosted PDS (Mode A/B) | The bridge/sim adapter |
| proto-blue | The bridge’s ATProto SDK (repo/XRPC/identity/lexicon/OAuth) | Running a PDS |
| skyauth | A narrow OAuth 2.1 client, or an SSRF/DPoP reference | Repo writes, firehose, PDS hosting |

**Aurora-Locus — best candidate for the PDS side of Mode A.**
Source-level check: `v0.10.0`, `MIT OR Apache-2.0`, depends on the
published `proto-blue = "0.3.3"`, and ships Docker/Caddy/systemd units.
It advertises the full PDS surface FIDOM needs: account/session/OAuth,
repo CRUD + MST, CAR export, firehose, DID:PLC/DID:Web identity, dynamic
lexicon loading, moderation, SQLite/Postgres, health/metrics. Open checks:
its `.env.example` says `PDS_DB_BACKEND=postgres` rejects at startup
pending chainlink #76, which contradicts the README’s “Postgres
first-class” claim — verify before relying on HA; federation, firehose,
and crawl are **off by default** (`PDS_FEDERATION_*`), so enable them for
public ATProto; conformance against the official PDS test suite;
strictness of `$type`/lexicon validation; loading custom `sh.fidom.*`
lexicons; firehose cursor/backfill; backup/migration story. It is young
and single-maintainer, so treat “production-ready” as a claim to test.

**proto-blue — best candidate for the bridge SDK.**
Source-level check: workspace `v0.3.3`, edition 2024, MSRV 1.88, dual
MIT/Apache-2.0; the README’s `v0.3.0` text lags the workspace version.
It covers the bridge’s ATProto primitives: validated DID/Handle/NSID/
AT-URI/TID/Datetime newtypes, DAG-CBOR/CID, lexicon validation, MST/CAR,
XRPC client, WebSocket/firehose, identity resolution, Agent/API, and
OAuth. This is the layer `server/atproto/` needs for BR-1..BR-12; it is
**not a PDS**. Open checks: the README states the project was generated
entirely by Claude Opus 4.6 and is pre-1.0, so pin a revision, run the
live-PDS tests, differential-test against the TypeScript reference, and
review the generated types/custom-lexicon path. Also verify its OAuth
client against the PDS you choose (DPoP/PAR behavior is PDS-specific).

**skyauth — narrow OAuth client, not a bridge.**
It specializes in OAuth 2.1 + DPoP + PAR + PKCE, SSRF/DNS-rebinding-safe
identity discovery, and Axum/Actix/Tower integration. The linked
`dollspace-gay/skyauth` is a fork at `v0.1.0` with 18 commits; the
upstream `mike10010100/skyauth` is materially ahead (`v0.3` README, 126
commits) and its `Cargo.toml` still points at the upstream repository.
Prefer upstream if this crate is selected. Also reconcile the published
crate name: the fork’s README badge points at `atproto-oauth`, while its
`Cargo.toml` package is `skyauth`. Use it only if its OAuth/SSRF
hardening is a hard requirement; otherwise use `proto-blue-oauth` so the
bridge has one auth stack. Open checks: upstream divergence, maintenance,
and whether its formal-verification claims cover the exact flows FIDOM
uses. It does not implement repo/XRPC/PDS.

**Correlated risk:** all three are from the same author, and Aurora-Locus
depends on proto-blue. Treat this as a supply-chain review item: pin
commits, vendor or lock dependencies, run the reference interop tests,
and keep the bridge able to switch PDS/SDK implementations.

## 14. Recommendation

1. **Do not implement a PDS, MST, CAR, or OAuth server from scratch.**
   Those are commodity protocol layers; the differentiated work is the
   FIDOM mapping, policy, queue, and sim boundary.
2. **Best single-language option:** evaluate **Aurora-Locus** as the
   Mode A PDS and **proto-blue** as the bridge SDK (`proto-blue-syntax`,
   `proto-blue-lexicon`, `proto-blue-repo`, `proto-blue-xrpc`,
   `proto-blue-identity`, `proto-blue-oauth`). Pin versions/revisions
   (`aurora-locus 0.10.x`, `proto-blue 0.3.3`) and run the official PDS
   conformance/interop fixtures before committing. Start with
   `listRecords` polling; add `proto-blue-ws` firehose later. For a
   public PDS, explicitly turn on Aurora’s federation/firehose/crawl
   flags and configure a real handle domain. Use **upstream**
   `mike10010100/skyauth` only as an OAuth/SSRF reference unless it
   cleanly replaces `proto-blue-oauth`; the `dollspace-gay` fork is
   behind it.
3. **Lowest-PDS-risk option:** run the official `bluesky-social/pds`
   and keep `proto-blue` (or `atrium`/`rsky`) as the Rust bridge client.
   If Aurora-Locus passes the conformance checks above, prefer it for a
   single Rust stack.
4. **Phase 2 (Mode B):** add OAuth/DPoP, firehose/Jetstream ingestion,
   identity rotation, moderation, and multi-PDS support. Re-evaluate
   `rsky`, `atproto-crates`, and proto-blue maturity then.
5. **Use the reference TypeScript PDS as the test oracle** even if the
   runtime PDS is Rust: run the same record fixtures through both and
   compare AT URIs/CIDs/validation behavior.
6. **Keep the bridge out of the sim crate.** New dependencies, HTTP,
   clocks, and persistence live in `server/atproto/`; `sim/` gains only
   the inbound `Verb`, external-mail provenance, and a drainable outbound
   event.

## 15. Explicit non-goals for the first bridge

- Building a general-purpose Bluesky AppView or full-network relay.
- Federating positional `say`, location, inventory, or host state.
- Executing ATProto attachments or federating trojans.
- Supporting multiple PDS implementations simultaneously at launch.
- Real-time chat; mail/board/profile are asynchronous by design.
