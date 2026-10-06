# Task Contract

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| JSON and JSON Schema 2020-12 | Closed Task status and recovery-class strings through Serde and the shared Vocabulary declaration |
| SurrealDB SDK 3.3.0 | Optional consumer-native delegation with the same literal-string kinds and nullable NONE profile used by Store |

## Ownership And Dependencies

This crate owns `TaskStatus` and `RecoveryClass`. Store persists them, Task Runtime
applies recovery and transitions, and Console describes them to the browser.
Status declaration order and the seven wire spellings are stable. Recovery declares
`resume`, `webhook_wait`, `provider_wait` and `interrupted_indeterminate` in that order.
The execution service owns the behavior of each recovery class.

The default dependency graph contains foundational types, serialization and schema
support. The `surreal` feature explicitly enables the SDK and the shared Vocabulary
native hook. Store selects that feature; contract consumers do not acquire a database
driver through the default profile. A separate package lets both Store and Task Runtime
reuse the declaration without a Store-to-runtime dependency cycle.

Task request, owner, failure and contribution envelopes have their existing owners.
This crate does not enumerate operation types or domain result payloads. Pure tests
cover complete vocabularies and unknown-string rejection. Native tests cover literal
values and optional NONE; Store's independent baseline checks kind and encoding
agreement with the persisted vocabulary.
