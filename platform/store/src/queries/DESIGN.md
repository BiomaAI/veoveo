# Store Queries

## Standards And Protocols

These statements use the pinned SurrealDB 3.3.0 SurrealQL profile and the Rust SDK's
native parameter bindings. They form an internal persistence interface.

## Query Placement

Store persistence operations and their colocated native test fixtures include complete
statements from this directory. Subfolders follow the calling source responsibility.
Artifact request scope, state and cursor combinations select complete static statements.
Knowledge completion and HNSW expansion windows do the same. Upload publication includes
one complete transaction with the registration body inside its authority guard.
The existing [observation queries](../../queries/changefeed/) stay beside their
[owning design](../changefeed/DESIGN.md).

Runtime values use driver bindings. Knowledge table names derive from typed GenerationId
UUIDs and enter table expressions through `type::table($chunk_table)`. INSERT uses a
bound native SDK Table value because its target grammar accepts a parameter but rejects
a function call. User names and test INFO table descriptors also use bindings.

The native grammar requires literals for HNSW dimensions and DEFINE USER passwords.
Generation creation substitutes only the checked integer dimension. User rotation
substitutes only a JSON-escaped secret strand and preserves redacted errors.
The observation replay template substitutes its checked numeric versionstamp and limit,
as described by its owner. These grammar exceptions accept no SQL body from callers.

Native Knowledge generation, search and index tests qualify table-expression execution;
user rotation and observation tests qualify the other dynamic slots. Parser validation
alone does not establish their database behavior.

## Artifact Access Deadlines

`PlatformStore::artifact_read_deadline` receives the checked `ArtifactReadScope` and
specific Artifact IDs. An absent selection covers the full collection; an empty selection
covers no members. The query selects candidate IDs before projecting deadlines through
`fn::kernel::artifacts::read_v1`. One database timestamp governs visibility and expiry
for every member. It applies no page limit and returns only the earliest admitted future
access deadline. The MCP caller caps that deadline at its token expiry.
