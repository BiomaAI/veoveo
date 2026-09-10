# Recording Hub

## Standards And Protocols

Hub implements the authenticated Recording ingest protobuf profile declared in
`docs/RECORDING_INGEST.md`. Durable payloads use the pinned Rerun RRD profile.
SurrealDB holds stream acceptance and materialization checkpoints. Local filesystem
journals use synchronized writes and atomic publication on the same filesystem.
`veoveo.io/recording-journal-quarantine/v1` is an internal JSON recovery receipt,
owned by Hub; it is not a producer protocol or an accepted recording batch.

## Terminal Journal Recovery

Startup replays accepted duplicate batches even when their stream is finished.
A journal at or beyond a terminal stream's authoritative `next_sequence` was never
accepted. Hub preserves it under `.ingest-journal/.quarantine/<tenant>/<stream>/`
and writes a receipt containing the terminal state, cutoff, sequence, exact journal
hash and byte length. A hard link avoids copying the payload. Both files and the
directory are synchronized before the original replay candidate is removed.
Interrupted publication resumes only when existing bytes and the receipt match.
Conflicts or failed durable publication retain the original and stop recovery.

Quarantine does not reopen a stream, advance a checkpoint, publish content, or
release storage. It remains charged to the spool's available-space floor. Hub logs
the stream and sequence for operator recovery; operators retain the files until an
explicit retention or export decision. Subsequent startup skips the private
quarantine directory. Accepted journals and mutable capture-layer recovery retain
the strict replay and ordering rules in `docs/RECORDINGS.md`.

The filesystem boundary tests cover interrupted publication, byte and receipt
conflicts, symlink rejection, accepted duplicate selection and terminal cutoff
selection. The installed recovery must also verify the exact bytes against the
receipt and the unchanged durable stream checkpoint.
