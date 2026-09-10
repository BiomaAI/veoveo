# Computers MCP Instructions

## Purpose

Follow root instructions, this component's DESIGN.md and `mcp/contract/DESIGN.md`
revision 3. Keep worker execution, protocol projection, grants and transport in
focused modules. The domain owns lifecycle state and fencing. Only this service
composes the domain with the private native provider runtime.

## Invariants

Never dispatch without a durable ticket and current action authority. A lost
ticket, provider reply, observer or Task lease permits observation only. A fake
preflight is test-only; production requires current policy and retained allocation.

## Build And Test

Run the application and domain store cases before protocol integration. The native
worker fixture owns its provider, allocator and private Docker daemon. Use the exact
qualified binaries and image; its synthetic policy is not installed-user evidence.
Record affected checks with `cargo xtask test-report` as required by root instructions.

## Contract Compliance

Contract revision: 3

The crate currently delivers the worker and shared application libraries. MCP discovery, domain tools,
resources, subscriptions, prompts/completion, well-known docs, service wiring and
packaging are not yet implemented. It is not registered or deployed as a hosted
server. Those explicit gaps must close before registration and release; worker
fixtures cannot serve as MCP conformance or installation evidence.
