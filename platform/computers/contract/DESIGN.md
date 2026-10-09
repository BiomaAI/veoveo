# Computers Contract

## Standards And Protocols

| Boundary | Profile |
|---|---|
| JSON and JSON Schema 2020-12 | camelCase object keys, including `resultUri`, and snake_case controlled values. Public Computer lifecycle, access, grants, execution, files and maintenance; generated Console and Workspace declarations use this owner schema |
| RFC 9562 | Distinct Computer, Task, grant, pairing, operation and provider identities with their declared UUID profiles |
| RFC 3339 | Grant, operation and maintenance timestamps; portable admission checks stated time relationships |
| RFC 3986 and RFC 6570 | Typed Computer result routes and discovery templates through the foundational resource components |
| Stock CLI loopback adapter | One-use `vcli1` pairing credential with canonical grant identity and lowercase 32-byte secret; explicit browser confirmation and loopback port 1024–65535 |
| Veoveo hosted MCP contract | Domain projections reuse these declarations; hosting, OAuth, gateway assertions and private provider transport belong to adapters |

## Ownership And Admission

This pure library owns the Computer vocabulary, identities, resource routes and
portable input/result checks. Closed unit values use the foundational Vocabulary
declaration; the owner preserves their snake_case spellings and variant order.
Payload variants keep their own closed camelCase fields. It has no Store, provider, HTTP or asynchronous runtime
dependency. The [domain service](../DESIGN.md) owns current grants, deadlines,
operation fencing and observed provider progress. The
[hosted worker](../../../servers/computers-mcp/DESIGN.md) composes those operations.

Automation grants require nonempty distinct permissions. Execute requires execution
limits, and other permission sets omit them. Execution permits 1–7200 seconds and
1–67108864 output bytes. Grant names are trimmed printable text of at most 64 bytes;
principal and client fields retain their owner syntax and additional field bounds.
Public construction and decoding share these checks. Selected native rows additionally
check references, authority snapshots and policy ceilings before exposing a view.

Collections check their Computer parent, child identities and declared size limits.
Granted access is a current service observation, never a portable authorization
claim. Expiry ceilings, registration, revisions, actor authority and session
permission are evaluated by the service. A stopping Computer may still hold a
command or file slot; its active execution and lifecycle Task need not be equal.

Pairing input checks the actual challenge code, name and loopback port profile.
Credential admission checks the producer's `vcli1` form, grant identity and secret
encoding. The token provides neither Debug nor Display and never enters a URL.
Public values carry no tenant, arbitrary provider endpoint or installer credential.

Pairing results bind the token’s admitted access grant identity to the visible grant ID and require a loopback callback port from 1024 through 65535. Construction and ordinary decoding apply the same checks; gateway forwarding consumes that admitted result. Token serialization preserves the one-use secret bytes, and formatting never exposes them.

## Current Wire And Private Profiles

Public producers and receivers share the checked owner DTOs. Retired snake_case
members are refused even when a current member is also present. Result construction
binds the resource address to its Computer, execution, transfer or grant identity;
ordinary deserialization repeats those checks before a projection can be forwarded.
Collections admit the declared parent and unique children. Execute permission and
its execution limits are admitted together; current authority stays with the service.

TerminalVersion admits only integer 2 during Rust construction and decoding.
Terminal version 2 uses the same camelCase control bodies, snake_case type values
and strictly increasing lease sequences. Ready reports authenticated acceptance of a
fresh SSH shell request and enables input. Attachments restore no previous shell or
output. Version 2 accepts only Ready and Lease server controls; producers and
consumers require a coordinated cut with existing attachments drained. Changing a
vocabulary declaration does not change terminal bytes or introduce another accepted
version. The Computer
collection continuation is a typed UUID position, with no serialized envelope or
new cursor version.

The domain's private signed authority, AEAD plaintext, AAD, HMAC, ticket and journal
profiles have distinct adapters. Native SurrealDB names and private execution/file
frames keep their declared spellings. Public naming does not authorize rewriting
retained homes, journals or encrypted pending operations. Host version 1, storage
version 1 and service version 3 keep their existing controlled bytes; OpenShell,
Docker and generated protobuf policy fields retain their upstream profiles.
