# HTTP Request Admission

## Standards And Protocols

| Standard or implementation | Supported role |
|---|---|
| HTTP semantics, RFC 9110 | Request status codes and content-type admission |
| JSON, RFC 8259 | JSON request bodies decoded by Axum and Serde |
| Workspace-pinned Axum and Serde | Body extraction, configured size limits and owner-defined deserialization |

## Ownership

`veoveo-http` provides `RequestJson<T>` for controlled HTTP request bodies in the
gateway composition, Artifact service, Console BFF, Agents and Workspace adapters.
Each owner defines its request types and closes controlled objects with Serde's
`deny_unknown_fields`. Explicit provider payloads and domain dictionaries keep
their declared shape. The extractor does not infer which fields are open.

This library has no MCP or domain dependency. Consumers gate it behind their HTTP
runtime features, keeping contract-only builds independent of Axum.

## Rejections

`RequestJson<T>` delegates decoding to Axum's `Json<T>` extractor. A JSON body that
does not fit `T` returns 400. An undeclared-key diagnostic names the key; other data
errors return a generic message to avoid reflecting submitted values. Syntax errors
return 400, oversized bodies return 413, and unsupported content types return 415.
Axum and the route's configured layers own body limits.

Serde's adjacent-tag enum decoder reports some undeclared keys as invalid values.
Those errors receive the generic 400 diagnostic. The extractor reads the typed
Serde error source and only recognizes an explicit unknown-field diagnostic; it
does not infer field names from arbitrary validation messages.

`AccessSubject` uses an owner-defined strict map decoder for its `kind` and `id`
fields. Its undeclared keys reach this extractor as structured unknown-field errors.
Malformed tag and identity values receive the generic diagnostic.

Response serialization continues to use Axum's `Json<T>`. Domain validation after
decoding keeps its owner-defined error response. Authentication remains in each
route's existing middleware or handler. Custom handlers that authenticate before
reading a body preserve that order and use the shared redacted diagnostic when
mapping their own JSON decode failures.

## Qualification

Router tests reject undeclared root and nested fields before entering a handler,
accept explicitly opaque payloads, and preserve syntax, media-type and size-limit
statuses. Owner route tests cover forwarding, authentication order and their error
envelopes.
