# Browser Contract Generation

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| JSON Schema 2020-12 | Schemars output from the owning Audit, Computers, Speech and MCP contract crates |
| TypeScript | Committed interfaces and aliases rendered by `json-schema-to-typescript` 16.0.0 |
| `tsType` | Converter-local extension for closed empty objects and impossible schemas; it is absent from the published schemas |

`cargo xtask release client-types` generates Audit, Console, Computers, agent-management, agent-control and
artifact-transfer, App catalog, cluster inventory and recording playback models under `apps/console/web/src/generated`, and Speech and Workspace models under
`apps/workspace/src/generated`. `--check`
compares those outputs without modifying them. The command imports only the pure
Computers contract, without the provider or domain-store dependency closure. The BFF
contract-only library selects App catalog and cluster inventory DTOs, and the Recording
contract supplies playback manifests.

Rust selects schema owners and fixed output paths. The Node converter reads one
schema from stdin and writes types to stdout. It requires the exact installed package
version and never downloads dependencies. All outputs are generated before any file
is changed. The package version was verified through the authoritative
[npm registry](https://registry.npmjs.org/json-schema-to-typescript/latest) on 2026-09-10;
the npm lockfile pins its integrity and dependency closure.

A closed empty JSON object becomes `Record<string, never>` because TypeScript's
empty interface also admits scalars. Named boolean schemas become equivalent object
schemas in the converter's input because its reference resolver requires object targets.
False definitions render as `never`; true definitions accept arbitrary values.
Agent-control models come from the operator-control DTOs in the MCP contract.
Artifact-transfer models cover upload policy, admission, status, part receipts, completed
receipts, access requests and share links. Browser camelCase presentation models derive
their fields from those generated HTTP shapes. Selected-file descriptors require a known
byte length because browser File selection supplies one; the upload protocol also admits
unknown-length consumers. Queue phases, file handles and persisted selection metadata
belong to the browser. The
canonical JSON schemas keep their original boolean definitions. A native Node fixture
compiles generated TypeScript and rejects assignments to impossible branches.
Runtime validators consume the original JSON schemas through
`CfWorkerJsonSchemaValidator` from `@modelcontextprotocol/client` 2.0.0. The shared
`apps/console/web/src/jsonSchema.ts` compiler interprets schemas without code generation
under the browser CSP. Its qualified profile covers references, boolean schemas,
nullable and closed objects, enums, UUIDs, timestamps, numeric bounds and composition.
Both clients use this compiler. Runtime tests reject impossible definitions and preserve
their nullable alternatives. No complete JSON Schema conformance is claimed.
