# Console Audit Readers

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| HTTP, RFC 9110 | Profile-scoped authenticated JSON reads, POST view admission and streaming JSON Lines export; responses use `no-store` |
| Server-Sent Events | Partition-filtered notifications and `Last-Event-ID` recovery through sealed block sequences |
| SurrealDB 3.3.0 | Parameterized SQL filtering and keyset pagination, partition-scoped LIVE queries, exact-record access receipts |
| Veoveo audit contract | Owner-defined scope, actor and target types, closed record summaries, queries and export framing |

`/admin/{profile}/console/audit` provides partitions, views, records, summary, stream
and export routes. The audit scope is checked on the acting principal. Its tenant
selects a tenant partition, while installation reads also require an administrator
or auditor role. Requests recheck token expiry, session validity and JWT revocation.
The browser's navigation hint never authorizes a request.

Opening a view commits one access record and returns its ID with a fifteen-minute
lifetime. Pages, daily summaries and the event stream reference that record. Store
checks its actor, profile, selected partition, activity and age in SQL. A missing or
expired receipt returns 410. The receipt is an identifier, not a credential, and
does not replace current authorization. Stream reconnects reuse the receipt.

Record filters run before SQL pagination. Record UUIDs order pages; cursors include
that order and the selected partition. Summaries read the grouped daily view.
The installation snapshot and inventory stream carry no audit records.

The stream registers partition-scoped record and block LIVE queries before reading
its checkpoint. Record changes invalidate the view immediately. Sealed block sequences
advance the reconnect cursor in commit order. Reconnect recovery reads only that
partition's blocks, with sixteen blocks per page and at most 64 pages per recovery.
Retention gaps reset the browser queries. Events carry invalidations and checkpoints;
the browser obtains rows from the authorized page endpoint. Catalog changes, token
expiry and the fifteen-minute connection limit close the stream. Each connection
holds a shared Console stream slot, limited to three per principal and sixteen total.

Export commits an access marker and waits up to ten seconds for sealing. Store then
captures the partition checkpoint and retention anchor in one transaction. The
download reads that retained interval in block commit order and applies content
filters in SQL. Its page cursor, page size and display order do not truncate or order
the export. A deletion from the frozen interval aborts the body. Session expiry,
catalog changes and a ten-minute deadline also abort it. The typed completion footer
distinguishes a finished download from a partial body. Exports share the stream slots.

The [consolidated plan](../../../../../../../../../docs/CONTRACT_CONSISTENCY_PLAN.md#unified-audit-log)
tracks qualification and startup of the sealer required by export.
