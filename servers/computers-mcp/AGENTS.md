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

## Public Library

Consumers select `default-features = false, features = ["contract"]`. Keep the
contract feature free of MCP integration, service, provider and asynchronous runtime
dependencies. Execution uses `runtime`; hosted endpoints and binaries require `mcp`.
Qualify the isolated consumer and both runtime feature configurations when changing
these gates. Public types keep their existing domain owners.

## Build And Test

Run the application and domain store cases before protocol integration. The native
worker fixture owns its provider, allocator and private Docker daemon. Use the exact
qualified binaries and image; its synthetic policy is not installed-user evidence.

## Contract Compliance

Contract revision: 3

The library implements shared lifecycle admission, its worker, and authenticated MCP
and HTTP projections. The executable uses validated installation configuration.
Browser and restricted stock CLI transport compose with the native runtime and share
current authority enforcement. Browser and stock CLI pairing are installed. Public
command admission and grant management have real-store/Artifact HTTP qualification.
Installed owner and agent file movement now pass public MCP and headed Console checks,
including exact bytes, retained environment updates and revoked access. The remaining
installed acceptance matrix stays active. Fixture evidence is not installed-user qualification.

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
- C11: met — command outputs and file transfers use actual-caller Artifact capabilities
- C12: met
- C13: met
- C14: met
- C15: met — digest-pinned OCI image and versioned Helm chart
- C16: met — installed typed registration declares routes, capabilities and policy
- C17: met — registration and crate documents declare contract revision 3
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
- C32: pending — installed docs pass K01–K06; K07/K08 do not apply to these collections; K09/K10 require owner qualification
