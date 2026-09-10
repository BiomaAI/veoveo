# Browser Contract Generation

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| JSON Schema 2020-12 | Schemars output from the owning Computers and MCP contract crates |
| TypeScript | Committed interfaces and aliases rendered by `json-schema-to-typescript` 16.0.0 |
| `tsType` | Converter-local extension for closed empty objects; it is absent from the published schemas |

`cargo xtask release client-types` generates the canonical Console and Computers
schemas and TypeScript models under `apps/console/web/src/generated`. `--check`
compares those outputs without modifying them. The command imports only the pure
Computers contract, without the provider or domain-store dependency closure.

Rust selects schema owners and fixed output paths. The Node converter reads one
schema from stdin and writes types to stdout. It requires the exact installed package
version and never downloads dependencies. All outputs are generated before any file
is changed. The package version was verified through the authoritative
[npm registry](https://registry.npmjs.org/json-schema-to-typescript/latest) on 2026-09-10;
the npm lockfile pins its integrity and dependency closure.

A closed empty JSON object becomes `Record<string, never>` because TypeScript's
empty interface also admits scalars. Other definitions use the converter's native
schema handling. Runtime validators consume the original JSON schemas with pinned
Zod 4.4.3. This selected `fromJSONSchema` profile is qualified for references, nullable
objects, required fields, closed objects, enums, UUIDs, timestamps and numeric bounds.
No complete JSON Schema validation implementation is claimed.
