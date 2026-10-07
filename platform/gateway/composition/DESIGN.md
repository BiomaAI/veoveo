# Gateway Composition

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Executable | Existing `gateway` CLI and `/usr/local/bin/gateway` image entrypoint; one process |
| MCP and HTTP | The [gateway library](../DESIGN.md) owns forwarding and authentication contracts; composition binds hosted routes and owner adapters |
| Installation plan | `veoveo.ai/module-selection/v1` and `veoveo.ai/module-plan/v1`, with camelCase JSON fields and digest-bound composition identity |
| Database | SurrealDB SDK and native isolated fixtures pinned to 3.3.0; owner lane admission through [module runner](../../modules/DESIGN.md) |
| Container | Existing `mcp-gateway` Bake image target, digest-pinned Debian base and non-root UID 10001 |

The audit CLI verification summary is an unversioned controlled camelCase object,
including `clockFindings`. Its embedded checkpoint uses the Audit owner's frozen
encoding. Record, block, checkpoint, export and crypto preimages keep that encoding;
the summary does not change them. Administrator control-plane and server-health
responses use camelCase. Native database projections and HTTP route capture names
remain explicit driver adapters.

## Ownership

`veoveo-gateway-composition` owns the `gateway` executable, installation commands,
HTTP route wiring and bindings to concrete owner libraries. `veoveo-mcp-gateway`
provides reusable authentication, catalog, forwarding and state mechanics. The
composition package supplies module schema exports and owner adapters; reusable
mechanics do not need an installation's optional module set to compile.

The executable keeps its CLI and image identity. `Dockerfile` copies the shared
builder's `gateway` artifact into the same entrypoint. Bake metadata names this
package when selecting the artifact. Package separation adds no process or deployment.

The command modules under `src/bin/gateway/module_installation` compose owner
exports into the generated plan. Migration and publication verify that plan against
the compiled composition before database effects. `installation-prepare` provisions
the selected namespace/database, claims its preparation generation and completes the
database-scoped runtime credential transaction. `module-migrate` applies one selected
owner lane after its prerequisites; `module-status` checks their current proof.
Connection startup applies no schema and refuses either mixed-catalog marker with
an actionable fresh-installation error. Optional owner schema comes from each owner's
declaration rather than a shared bootstrap catalog.

## Query Files

Composition statements live in `src/queries/` and command-process fixtures in
`tests/queries/`. Installation commands bind namespace and database names through
SurrealDB's identifier parameter slots. Marker-refusal fixtures bind their table
names and select records through `type::table`; each inspection returns database
metadata, table metadata and rows in that order.

The [malformed numeric-parameter fixture](tests/queries/observation_declarations/malformed_show_numeric_parameters.surql)
uses unsupported `SHOW CHANGES` parameter slots. Its native test requires parser
rejection; successful query validation does not apply to that fixture.

## Qualification

Ordinary executable tests accompany their route modules. The binary integration
harnesses under `tests` exercise the real command process and preserve their declared
native prerequisites. Reusable library integration tests stay in the parent package.
Image selection tests prove the Bake target builds this package and includes the
kernel library through its normal dependency graph.

The schema-lanes harness compares two independently fresh full compositions. Typed
captures include every table's fields, indexes, events, child tables and empty LIVE
map, alongside database table/function/analyzer definitions and checked table
ownership. Field DDL preserves types, assertions and record-reference policies.
Separate field and index mutations in the second fixture must change the comparison
while its outer inventory and ownership stay equal. Independent capture controls
also require the comparison to detect event, function, analyzer and ownership drift.
Runtime credentials, namespace
names and migration receipt timestamps describe installation instances; they are
outside this schema capture. No captured schema definition is normalized or omitted.

Installed lifecycle qualification belongs to [deployment smoke](../../../testing/deployment-smoke/DESIGN.md).

## Optional HTTP Routes

Composition supplies the shared HTTP context and declares owner factories keyed by
`ModuleName`. Computers, Speech, Recording, Agents and Workspace own their handlers.
Factory construction is deferred until binding validation succeeds. The executable
retains the module cleanup supervisor across construction and serving, closes
admission before shutdown and reports unresolved cleanup within one module deadline.
Listener setup failures and route composition panics await the same cleanup before
returning. Module workers drain before the process closes audit delivery.
Authenticated server health lists module bindings separately from backend probes.

## Owning Smoke Delivery

The nondefault `smoke` feature enables `gateway-smoke` and `gateway-smoke-support`. Gateway assertion source sits beside this composition because it validates the selected production registry, gateway authorization and installed component relationships. `smoke_support` holds these composition-specific fixtures; the shared testing support owns only transport, process, artifact and framework mechanics. The suite invokes the existing Bioma delivery for assertions that compose additional domain processes, without introducing a dependency from a production domain back into a composition.

The composed schema exporter preserves all 52 filenames used by offline bundles and owning schema tests. Native child paths are selected from Cargo compiler messages, verified by package/target/features and content digest, and supplied through the dispatch artifact manifest. An explicit binary path must identify that same observed artifact.

The Audit keygen CLI writes a closed camelCase public summary with `keyId` and
`publicKey`. The seed file, key identity, raw public key and permissions use the
Audit owner crypto profile. This stdout wrapper is separate from frozen Audit
record, block, checkpoint and export bodies.
