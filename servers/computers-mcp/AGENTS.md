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

The library implements shared lifecycle admission, its worker, and authenticated MCP
and HTTP projections. The executable uses validated installation configuration. Browser grant transport is composed with the native runtime. Public Console/CLI access, agent execution,
file movement, catalog registration and packaging remain active work. Fixture evidence is not installed-user qualification.

- C01: met
- C02: met
- C03: met
- C04: met
- C05: met
- C06: met
- C07: met
- C08: met
- C09: met
- C10: met
- C11: pending — governed Artifact execution/file tools remain delivery work
- C12: met
- C13: met
- C14: met
- C15: pending — OCI/Helm packaging remains delivery work
- C16: pending — installed typed control-plane registration remains delivery work
- C17: pending — crate revision is declared; installation registration is pending
- C18: met
- C19: met
- C20: met
- C21: met
- C22: met
- C23: met
- C24: met
- C25: met
- C26: met
- C27: met
- C28: met
- C29: met
- C30: met
- C31: pending — installed readiness against the declared catalog is pending
