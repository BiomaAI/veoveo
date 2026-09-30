# Browser Contract Generation

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| JSON Schema 2020-12 | Schemars output from the owning Computers, Speech and MCP contract crates |
| TypeScript | Committed interfaces and aliases rendered by `json-schema-to-typescript` 16.0.0 |
| `tsType` | Converter-local extension for closed empty objects and impossible schemas; it is absent from the published schemas |

`cargo xtask release client-types` generates Console, Computers and agent-management
models under `apps/console/web/src/generated`, and Speech and Workspace models under
`apps/workspace/src/generated`. `--check`
compares those outputs without modifying them. The command imports only the pure
Computers contract, without the provider or domain-store dependency closure.

Rust selects schema owners and fixed output paths. The Node converter reads one
schema from stdin and writes types to stdout. It requires the exact installed package
version and never downloads dependencies. All outputs are generated before any file
is changed. The package version was verified through the authoritative
[npm registry](https://registry.npmjs.org/json-schema-to-typescript/latest) on 2026-09-10;
the npm lockfile pins its integrity and dependency closure.

A closed empty JSON object becomes `Record<string, never>` because TypeScript's
empty interface also admits scalars. Named boolean schemas become equivalent object
schemas in the converter's input because its reference resolver requires object targets.
False definitions render as `never`; true definitions accept arbitrary values. The
canonical JSON schemas keep their original boolean definitions. A native Node fixture
compiles generated TypeScript and rejects assignments to impossible branches.
Runtime validators consume the original JSON schemas with pinned
Zod 4.4.3. This selected `fromJSONSchema` profile is qualified for references, nullable
objects, required fields, closed objects, enums, UUIDs, timestamps and numeric bounds.
The shared `apps/console/web/src/jsonSchema.ts` compiler presents equivalent object
forms for named boolean definitions under the root `$defs`; Zod's reference lookup
otherwise treats `false` as missing. Both clients use this compiler. Runtime tests
reject every value for impossible definitions and preserve their nullable alternatives.
No complete JSON Schema validation implementation is claimed.
