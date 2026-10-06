# Computers Contract

## Standards And Protocols

| Boundary | Profile |
|---|---|
| JSON and JSON Schema 2020-12 | Public Computer lifecycle, access, grants, execution, files and maintenance; generated Console and Workspace declarations use this owner schema |
| RFC 9562 | Distinct Computer, Task, grant, pairing, operation and provider identities with their declared UUID profiles |
| RFC 3339 | Grant, operation and maintenance timestamps; portable admission checks stated time relationships |
| RFC 3986 and RFC 6570 | Typed Computer result routes and discovery templates through the foundational resource components |
| Stock CLI loopback adapter | One-use `vcli1` pairing credential with canonical grant identity and lowercase 32-byte secret; explicit browser confirmation and loopback port 1024–65535 |
| Veoveo hosted MCP contract | Domain projections reuse these declarations; hosting, OAuth, gateway assertions and private provider transport belong to adapters |

## Ownership And Admission

This pure library owns the Computer vocabulary, identities, resource routes and
portable input/result checks. It has no Store, provider, HTTP or asynchronous runtime
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
