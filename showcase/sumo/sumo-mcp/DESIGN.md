# SUMO MCP Design

## Standards And Protocols

The server implements the Veoveo hosted MCP contract revision 4 over MCP 2026-07-28 Streamable HTTP. JSON resources and Schemars-generated tool arguments describe traffic state and controls. Forwarded gateway assertions authenticate the installation profile and server audience. SUMO TraCI supplies the simulation control protocol; Rerun carries recording products. The knowledge-source extension describes embedded owner documentation, with installed collection qualification declared separately in the owner profile.

## Library Profiles

The `contract` feature exposes the current traffic DTOs and `SumoTaskKind` with
defaults disabled. Its normal/build graph excludes MCP, Task services, TraCI,
Recording, Rerun and asynchronous runtime implementations. `runtime` selects the
driver and recording adapter. The default `mcp` profile adds the hosted server
and preserves its operational dependencies; the binary requires that feature.

Public traffic DTO fields use camelCase and refuse retired snake_case, mixed and
unknown fields. The generated input/output schemas, resource JSON and Task results
use these same owner types. Task names and the private durable operation envelope
keep their declared snake_case forms; TraCI, SUMO XML and Rerun formats belong to
the external adapters.

The producer and Bioma receiver require a coordinated installation drain. Stop
new SUMO mutations, settle outstanding Tasks and consume their terminal results
with the matching receiver before replacing the server and receiver together.
A rolling overlap of old and current DTO writers is unsupported. Missing or retired
fields fail admission without aliases or inferred conversion. Retained old Task
results require their original receiver before the coordinated replacement; this
cut does not rewrite durable histories.

## Runtime And Ownership

The server holds one simulation driver and publishes its recording through the configured recording proxy. Live controls share that driver. Durable offline operations use the shared Task runtime and Artifact plane, with recorded operation identity and recovery. The server keeps gateway authentication on both MCP and administrative routes. The simulation driver and GPU/provider acceptance keep their existing prerequisites.

## Documents And Compliance

The owner authors one complete contract-compliance.json for catalog revision 2 and hosted contract revision 4. Generated marked sections in AGENTS.md project that checked profile. The binary embeds the exact manual/design bytes and verifies profile identity and projection before serving them. A declaration is an owner statement; runtime conformance results remain separate.

The authenticated well-known surface serves sumo://docs, document members and sumo://contract. Docs member templates carry the shared knowledge collection metadata and conditional-read observations. The administrative mount serves the same embedded bodies and llms.txt index through the same gateway token verifier. Domain resources continue to use the simulation owner. Installed K01–K10 and the other unqualified requirements stay pending in the profile.

## Verification

Owning Rust tests exercise signed administrative document reads, missing/foreign identity refusal, actual embedded document bytes and complete declaration admission. Protocol tests reuse the production documentation adapter and the shared stateless transport without starting a simulator, provider, cluster or GPU process. Simulation and visual acceptance run through their separately declared hardware workflows.

Startup admits the embedded profile and manual and compares C32 with the same
Discover configuration used by the MCP handler before simulator connection,
Recording publication, Task connection or recovery. Rejection never enters the
owner startup continuation. The owning regression qualifies that ordering with
synthetic effects; it does not execute simulation or GPU work.

The owning naming controls qualify all nine changed DTO families, required fields,
retired/mixed refusal and the complete generated tool catalog. Safe local resources
and Task products pass the maintained C33 inspector through their production
serialization helpers. The Bioma receiver admits those owner DTOs and constructs
current control requests. These are source and local behavioral checks; installed
naming, simulation and hardware qualification remain pending.
