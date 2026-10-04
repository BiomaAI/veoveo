# Gateway Composition

Keep the `gateway` entrypoint thin and put command and route behavior in focused
modules. Bind concrete owner adapters here; reusable gateway mechanics belong in
the parent library. Preserve the executable CLI, image entrypoint and separate
installation/runtime credentials. Schema-only composition dependencies must not
activate domain runtimes merely to enumerate declarations.

Binary process tests keep their isolated native fixtures, timeouts and owned cleanup.
Use the existing Bake `mcp-gateway` target and shared Rust artifact builder.
