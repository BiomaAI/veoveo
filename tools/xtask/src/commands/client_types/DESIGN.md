# Browser Contract Generation

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| JSON Schema 2020-12 | Schemars output from the owning Audit, Computers, Speech, Agents, Workspace and MCP contracts |
| TypeScript | Committed interfaces and aliases rendered by `json-schema-to-typescript` 16.0.0 |
| `tsType` | Converter-local extension for closed empty objects and impossible schemas; it is absent from the published schemas |

`cargo xtask release client-types` generates Audit, Console, Computers, agent-management, agent-control and
artifact-transfer, App catalog, cluster inventory and recording playback models under `apps/console/web/src/generated`, and Speech and Workspace models under
`apps/workspace/src/generated`. `--check`
compares those outputs without modifying them. The command imports only the pure
Computers contract, without the provider or domain-store dependency closure. The BFF
contract-only library selects App catalog, cluster inventory and installation
snapshot/event DTOs, and the Recording
contract supplies playback manifests.
Agents' pure authoring contract supplies agent-management models. Workspace's
`app-contract` profile supplies its DTOs and native RMCP App envelopes. The Workspace
pure contract does not enable that SDK profile.

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
Agent-control models come from the operator-control DTOs in the Agents contract.
Artifact-transfer models cover upload policy, admission, status, part receipts, completed
receipts, access requests and share links. Browser camelCase presentation models derive
their fields from those generated HTTP shapes. Selected-file descriptors require a known
byte length because browser File selection supplies one; the upload protocol also admits
unknown-length consumers. Queue phases, file handles and persisted selection metadata
belong to the browser. The
canonical JSON schemas keep their original boolean definitions. A native Node fixture
compiles generated TypeScript and rejects assignments to impossible branches.
Runtime validators consume the original JSON schemas through
`CfWorkerJsonSchemaValidator` from `@modelcontextprotocol/client` 2.3.1. The shared
`apps/console/web/src/jsonSchema.ts` compiler interprets schemas without code generation
under the browser CSP. Its qualified profile covers references, boolean schemas,
nullable and closed objects, enums, UUIDs, timestamps, numeric bounds and composition.
Both clients use this compiler. Runtime tests reject impossible definitions and preserve
their nullable alternatives. No complete JSON Schema conformance is claimed.

The Console BFF bundle selects installation snapshot, detail and entity-specific
SSE roots. Gateway Contract supplies bootstrap declarations through that bundle.
The Artifact transfer bundle includes its upload notification profile; browser
queue state does not contribute wire schemas.

The same generator selects the View, Stream, Timeseries, UAV and Map owner App
bundles under each server's `app/generated` directory. Its Workbench selection
combines the Apps configuration schema with the actual Recording projection handle.
These selections belong to tooling composition. Domain routes and result relationships
stay in the consuming server, and the browser admission package stays domain-neutral.
