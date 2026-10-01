# Audit Writer

Follow root AGENTS.md and docs/AUDIT.md. The contract crate owns wire types. Store owns
SurrealQL and driver conversion. This crate owns batching, signing, sealing, export and
verification. Preserve each caller's commit acknowledgement. Never move required audit
writes behind response delivery. Domain writes append inside the existing transaction.
Keep signing keys redacted and separate from gateway
assertion keys.
