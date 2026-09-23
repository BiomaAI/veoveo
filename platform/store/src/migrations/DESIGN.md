# Store Migrations

## Standards And Protocols

SurrealDB 3.2.4 executes SurrealQL transactions over the existing authenticated
WebSocket client. SHA-256 binds applied migration SQL to compiled source. Migration
catalogs are internal Rust values, with no runtime package or registration protocol.
The upstream history format keeps its existing numeric identity and SQL checksums.
The downstream history uses a separate schemafull table with root-only access.

## Catalogs And Ordering

Upstream migrations live in `platform/store/migrations`. Fork migrations live in
`platform/store/downstream`, whose `catalog.rs` is empty upstream. Each lane starts
at zero and advances contiguously. A downstream migration includes its required
upstream version. Requirements must exist in the compiled upstream catalog and cannot
move backward across the downstream sequence.

The runner validates both histories before applying pending changes. It first applies
upstream changes, then pending downstream changes. Each migration body and its history
record commit in one transaction. Stored downstream name, filename, SQL checksum and
upstream requirement must match the compiled declaration. Unknown versions, missing
history and drift fail before pending migrations run. Root credentials are required.

Two replicas applying the same migration can accept a concurrent winner only after
reading its matching committed history. A transaction error without that observation
remains an error. Existing upstream preparation for online indexes keeps its owner.

## Upstream Integration

Separate histories prevent numbering collisions. They do not prove that a fork's SQL
is compatible with a later upstream change. The fork owner reviews the upstream diff,
adapts dependent code, and qualifies the merged runner against representative retained
data. Pending downstream migrations run after all compiled upstream migrations; a
change that must prepare data before an upstream step requires an explicit coordinated
maintenance procedure before that upgrade.

Migration catalogs are append-only after deployment. Correct mistakes with new
migrations. Retiring an applied entry or renumbering it is drift. Deploying a binary
with an older catalog to a newer database fails schema validation. Mixed-version
migration jobs are unsupported: drain the previous migration owner before upgrading.
Ordinary service rollout follows the data compatibility of the qualified release.

## Recovery And Qualification

A failed transaction rolls back its body and history record. The additive history-table
migration leaves application data and prior upstream history unchanged. An image rollback
does not undo committed schema changes; choose a forward repair or restore an
installation-owned database snapshot according to the change's recovery plan. This
runner creates no backup service and performs no automatic destructive conversion.

Pure tests cover catalog and history rejection. Isolated SurrealDB tests cover a fork
migration followed by an upstream advance, equal numeric versions across lanes,
concurrent attempts, transaction rollback, and rejection before pending side effects.
Tests own disposable database names, timeouts and cleanup. These cases use the same
executor as `PlatformStore::migrate`.
