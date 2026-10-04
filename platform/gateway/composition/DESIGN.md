# Gateway Composition

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| Executable | Existing `gateway` CLI and `/usr/local/bin/gateway` image entrypoint; one process |
| MCP and HTTP | The [gateway library](../DESIGN.md) owns forwarding and authentication contracts; composition binds hosted routes and owner adapters |
| Installation plan | `veoveo.ai/module-selection/v1` and `veoveo.ai/module-plan/v1`, with camelCase JSON fields and digest-bound composition identity |
| Database | SurrealDB SDK and native isolated fixtures pinned to 3.3.0; owner lane admission through [module runner](../../modules/DESIGN.md) |
| Container | Existing `mcp-gateway` Bake image target, digest-pinned Debian base and non-root UID 10001 |

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
the compiled composition before database effects. Installation preparation still
runs the explicit mixed-schema prologue; this package does not redistribute its SQL.

## Qualification

Ordinary executable tests accompany their route modules. The binary integration
harnesses under `tests` exercise the real command process and preserve their declared
native prerequisites. Reusable library integration tests stay in the parent package.
Image selection tests prove the Bake target builds this package and includes the
kernel library through its normal dependency graph. Installed lifecycle qualification
belongs to [deployment smoke](../../../testing/deployment-smoke/DESIGN.md).
