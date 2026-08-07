# MCP Conformance Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| Model Context Protocol | negotiated Streamable HTTP protocol plus the hosted-server requirements selected by a typed profile |
| JSON-RPC 2.0 | MCP request and response envelopes |
| JSON Schema 2020-12 | self-contained tool input schemas and generated profile/report schemas |
| OAuth 2.0 protected-resource metadata | unauthenticated Bearer rejection checks selected by the profile |
| `veoveo.io/mcp-conformance-profile/v1` | domain-neutral declaration of applicable hosted-server checks |
| `veoveo.io/mcp-conformance-report/v1` | machine-readable implementation identity, capabilities, requirement results, and evidence |
| `veoveo.io/hosted-mcp/v1` | initial Veoveo hosted-server contract revision |
| `veoveo.io/live-view/v2` | optional provider-neutral authoritative camera, bounded viewer-product, actor/browser lease, signaling, and redaction profile layered on a domain-owned simulation server |

## Boundary

This crate certifies a running MCP server through public HTTP and MCP surfaces. The
library accepts a typed profile and credentials supplied out of band, then returns a
typed report. The CLI reads and writes the same JSON contracts.

The crate depends on shared MCP protocol and Veoveo contract infrastructure. It does
not depend on a domain server, showcase, example, extension implementation, or client
repository. Domain lifecycle smoke remains with the component that owns the domain.

## Profile

A profile names the expected implementation slug, selected contract revision, allowed
resource URI schemes, HTTP boundary checks, and required, optional, or forbidden MCP
surfaces. Required tool, resource, template, and prompt identities are extension-owned
inputs rather than compiled registry entries. A hosted-server certificate selects
exactly `veoveo.io/hosted-mcp/v1`: resources are required, and the profile must name
the administrative `llms.txt` URL. Unauthenticated Bearer rejection is required for
the MCP endpoint. C18–C21 cannot be disabled by a profile.

Credentials never enter the profile or report. The CLI receives a bearer through
`MCP_BEARER_TOKEN`, or uses the existing direct-hosted internal assertion arguments
for installation-local acceptance. The same credential authenticates the MCP endpoint
and the administrative docs projection. Profile validation requires both URLs to have
the same scheme, host, and effective port before any credential can be forwarded.
Certification also requires the index and every linked document to return HTTP 401
without that credential.

## Authoritative Live-View Profile

A simulation profile may require `list_live_cameras`, `open_live_view`,
`renew_live_view`, and `close_live_view` plus domain-owned camera, product, and
redacted lease resources. The profile verifies strict schemas, stable physical-slot identity,
actor and browser-instance isolation, token rotation, credential redaction, typed
capacity rejection, App declaration, and authenticated signaling. Resource URIs retain
the simulation server's own scheme; conformance never requires a shared renderer URI.

The anonymous external simulation fixture exercises this public contract without
claiming visual or GPU acceptance. Hardware RTX rendering, one NVENC session per active
camera, multi-peer encoded fan-out, frame freshness, and headed-browser playback remain
implementation-owned evidence. The first-party UAV simulation acceptance supplies that
evidence for the NVIDIA runtime.

## Report

Each result carries a stable requirement identifier, status, summary, and bounded
evidence. The report records the negotiated protocol, implementation identity,
advertised capabilities, selected contract revision, and execution interval. A failed
requirement produces a report and a non-zero CLI exit.

Certification reads the live contract declaration and binds it to the selection and
observation. The selected revision must equal the conformance client's supported
revision. The declaration's numeric revision must be the numeric member of that
revision, its server must match both the expected slug and initialized implementation,
and its stable capability inventory must match the MCP lists according to the normative
contract. The client follows the relative document links published in `llms.txt`;
it does not synthesize document URLs from parsed identifiers.

## CLI Output

Each CLI command reserves standard output for its requested result. Structured resources
therefore remain parseable even when the server emits notifications while the command is
running. Unsolicited progress, task-status, resource-update, and list-change notifications
are operator diagnostics on standard error.

## Distribution

The thin `certify` binary is copied into the digest-addressed
`veoveo/mcp-conformance` OCI image. The image contains no server implementation and
runs as uid 10001. Installation operators mirror it into their private registry or
offline bundle and execute it against extension endpoints.
