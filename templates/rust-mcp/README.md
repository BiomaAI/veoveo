# Rust MCP server template

This crate is the template to copy for a Rust MCP server hosted inside a Veoveo
installation. It is a complete server, `glossary`, that answers what a Veoveo
hosting term means. The shared host in `veoveo_mcp_contract::hosting` supplies
transport, authentication, discovery, documents and shutdown, so the template
holds only its contract, its setup and its domain.

## Where each part lives

| Part | File | What to change |
|---|---|---|
| Library features | `Cargo.toml`, `src/lib.rs` | Keep `contract` free of MCP and runtime dependencies; gate engine, Store and provider crates under `runtime` |
| Identifiers and addresses | `src/contract.rs` | Replace `TermId` and `GlossaryResource` with the new domain's types |
| Domain logic | `src/glossary.rs` | Replace with the domain; keep MCP types out of it |
| Configuration and startup | `src/bin/server.rs` | Port, service name, and any domain state to build before `HostedServer` |
| Checked setup | `src/bin/server/setup.rs` | Slug, scheme, capabilities, resources and templates |
| MCP surface | `src/bin/server/handler.rs` | Tools in `#[tool_router]`, `read`, prompts and completion |
| Tests | `src/bin/server/tests.rs` | Keep one test per surface, through `TestGateway` |
| Agent manual and design | `AGENTS.md`, `DESIGN.md` | Embedded at build time and served at `{scheme}://docs` |

## Creating a server from this template

1. Copy this directory to `servers/<domain>-mcp` and add it to the workspace
   members. Rename the package to `veoveo-<domain>-mcp`, the binary to
   `<domain>-mcp`, and the slug and scheme in `contract.rs` and `setup.rs`.
2. Replace the contract types, the setup declarations and the domain. Every
   resource descriptor comes from a typed address; keep that rule.
3. Add what the domain needs from the reference servers:

   | Need | Reference |
   |---|---|
   | Durable tasks | `servers/duckdb-mcp`: a `DurableTaskService` wrapped in `DurableTasks::tasks_only` |
   | Resource subscriptions | `servers/frames-mcp`: `ResourceSubscriptions` with `DurableTasks::with_resources` |
   | A tool result that creates a product | `hosting::product_result`, which checks `resultUri` against the link |
   | Content that must not be reused | `DomainRead::no_store` |
   | A signed provider callback | `servers/media-mcp`: `public_routes` |

4. Rewrite `AGENTS.md` and `DESIGN.md` for the domain, including each
   `Contract Compliance` item.
5. Add the image, Helm workload and gateway registration using an existing server's
   patterns. Installation policy owns exposure, scopes and endpoints.

The [hosting procedure](../../mcp/contract/DESIGN.md#hosting-a-server) states what
the host does for every server.

## Build and test

```sh
cargo test -p veoveo-glossary-mcp
cargo test -p veoveo-glossary-mcp --no-default-features --features contract
cargo clippy -p veoveo-glossary-mcp --all-targets -- -D warnings
```

The tests run the complete hosted router in-process with signed gateway tokens and
need no deployment. Run the server locally with a test JWKS:

```sh
cargo run -p veoveo-glossary-mcp -- \
  --public-base-url http://localhost:8810 --allow-loopback-hosts \
  --internal-trust-jwks "$VEOVEO_INTERNAL_TRUST_JWKS"
```
