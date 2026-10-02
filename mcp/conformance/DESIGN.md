# MCP Conformance Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| Model Context Protocol | negotiated Streamable HTTP protocol plus the hosted-server requirements selected by a typed profile |
| JSON-RPC 2.0 | MCP request and response envelopes |
| JSON Schema 2020-12 | bounded tool input schemas with same-document references and composition, plus generated profile/report schemas |
| OAuth 2.0 protected-resource metadata | unauthenticated Bearer rejection checks selected by the profile |
| OAuth 2.0 client credentials / RFC 7523 | RS256 private-key client assertions with explicit installation key identity; redirects are rejected |
| `veoveo.ai/mcp-conformance-profile/v1` | domain-neutral declaration of applicable hosted-server checks |
| `veoveo.ai/mcp-conformance-report/v1` | machine-readable implementation identity, capabilities, requirement results, and evidence |
| `veoveo.ai/hosted-mcp/v3` | Veoveo hosted-server contract revision for MCP `2026-07-28` |
| `ai.veoveo/knowledge-source` | typed collection declarations, enumeration, observations and conditional member reads; [extension rules](../knowledge-extension/DESIGN.md#server-rules) |
| `veoveo.ai/live-view/v4` | optional provider-neutral authoritative cameras, typed camera regions in shared encoded products, actor/browser authorization, Annex B H.264 WebSocket fanout, and redaction profile layered on a domain-owned simulation server |

## Boundary

This crate certifies a running MCP server through public HTTP and MCP surfaces. The
library accepts a typed profile and credentials supplied out of band, then returns a
typed report. The CLI reads and writes the same JSON contracts.

The hosted runner uses shared MCP protocol and Veoveo contract infrastructure without
a compiled registry of domains. Other protocol utilities can consume server-owned
contract features. Domain lifecycle smoke and database selection tests belong to the
component that owns the domain.

Certification, `info`, `tools` and `apps-check` traverse their catalog pages through
the same typed reader. Each surface permits at most 1,024 pages and 16,384 items within
30 seconds. Repeated cursors fail the read. Required tools, schemas and App links are
checked across the complete traversal, including descriptors on later pages.
`tools` reads and validates the complete tool catalog. It does not request prompts or
resource templates. Domain harnesses use it to require the tools their scenario calls;
`info` reads all three catalogs and continues to require their availability.

The `modular_server` integration test hosts the independent
[`modular-mcp` fixture](../../testing/fixtures/modular-mcp/DESIGN.md), which is a
development dependency. The fixture owns its scopes and resource family in a library
with an isolated contract feature. Its MCP feature implements the public setup trait.
The test supplies a profile to this same hosted runner, then checks typed reading
access and scope denial separately. A 60-second deadline and owned loopback listener
bound the test. It uses synthetic credentials and requires no cluster or GPU.

Repository checks discover hosted servers under `servers/*-mcp` and include the
Python server template. Each manual declares C01–C32; a pending item states its
qualification gap. C18–C21 must be met for every hosted server.

Tool input schemas retain the ordinary SDK representation. Conformance permits
same-document references and composition, rejects external references without fetching
them, and applies per-document limits of 1 MiB, depth 64, 50,000 nodes, 4,096 references,
and 4,096 composition branches before meta-schema validation.

## Profile

A profile names the expected implementation slug, selected contract revision, allowed
resource URI schemes, HTTP boundary checks, and required, optional, or forbidden MCP
surfaces. Each profile lists its required tool, resource, template, and prompt
identities; the conformance client has no compiled registry of them. A hosted-server certificate selects
exactly `veoveo.ai/hosted-mcp/v3`: resources are required, and the profile must name
the administrative `llms.txt` URL. Unauthenticated Bearer rejection is required for
the MCP endpoint. C18–C21 cannot be disabled by a profile.

Credentials never enter the profile or report. The CLI receives a bearer through
`MCP_BEARER_TOKEN`, or uses the existing direct-hosted internal assertion arguments
for installation-local acceptance. The same credential authenticates the MCP endpoint
and the administrative docs projection. Profile validation requires both URLs to have
the same scheme, host, and effective port before any credential can be forwarded.
Certification also requires the index and every linked document to return HTTP 401
without that credential.

Direct-hosted assertions carry an automated service request context whose client ID
matches the selected principal subject. The context binds the tenant, Work Context,
scopes and thirty-minute expiry through the shared issuer's validation. Its source
issuer and resource are `https://conformance.veoveo.local`; it has no browser session
family or delegation. This synthetic identity supports direct protocol certification.
Public OAuth and current installation policy require separate installed acceptance.

## Knowledge Checks

`src/runner/knowledge.rs` runs when Discover declares the knowledge-source extension.
K01 rejects unsupported settings, duplicate collections and unowned URI schemes.
K02 requires the shared immutable docs descriptor and matching member template.
K03 traverses at most eight pages of 100 members within 30 seconds; a larger
qualification fixture fails explicitly. It rejects repeated cursors and member URIs.
The required docs check also verifies document-ID ordering across every docs page.

K04–K06 allow 60 seconds per collection. Each member must return one admitted text
or JSON item within 64 KiB, a content-bound observation and an empty response to its
matching validator. An unauthenticated conditional HTTP read must return 401.
The independent fixture also checks conditional denial for an authenticated caller
without its required scope. These checks do not qualify arbitrary domain access
policies; domain tests own those cases.

K07 and K08 skip collections without change subscriptions or search declarations.
Every declared listen collection and search tool requires one owner-supplied probe.
`knowledge_probes` exposes typed selections, `KnowledgeChangeDriver` for existing
members and `KnowledgeCreateDriver` for members assigned an identity on creation;
`run_hosted_server_conformance_with_probes` executes them in the hosted report.
The ordinary runner and CLI fail these checks when a required probe is absent.
They never select domain mutations or restart commands from server names.

For K07 updates and removals, the owner supplies populated members, mutations and a restart operation.
The runner requires acknowledgement of member and collection subscriptions, waits
for both initial observation invalidations, then performs the mutation. Separate
notifications and a changed revision prove mutation delivery. It checks that text and access survive the
restart. It subscribes again and verifies another committed change. Each probe has
90 seconds, with 15 seconds for each notification window. The owner must restart the
actual service or source and preserve only declared persistence. Database recovery,
cross-replica delivery and policy cases belong to that owner's harness. Subscription
handles cancel on failure or timeout, and the owner cleans up its fixture.

`KnowledgeChange::Update` changes the same member twice or uses distinct members
before and after restart. With distinct members, the second must survive the first
change and restart unchanged, and the first must preserve its committed state after
the second change. `KnowledgeChange::Remove` requires two distinct members. Each
removal must notify both subscriptions, disappear from enumeration and reject full
and conditional reads. The first removal must survive restart, while the second
member must remain unchanged until its own removal.

`KnowledgeChange::Create` subscribes to the collection and waits for its initial
invalidation before creating a member. The owner returns a typed `ResourceUri`.
The checker requires a new identity, a separate collection invalidation, enumeration,
a content-bound observation and a matching conditional read. The same member must
remain readable and enumerated after restart with unchanged content and access. A
fresh subscription then observes a second distinct creation, and the first member
must stay unchanged. Generated member identities need no advance subscription.
Creation uses the same 90-second deadline and 15-second notification windows.
Each variant contains its applicable driver, preventing invalid driver selections.

For K08, the owner supplies tool arguments, expected hits and a second authenticated
reader whose expected results are a strict subset, or whose scopes deny the entire
tool. `KnowledgeSearchAccess` makes that policy expectation explicit. The runner checks the closed
`SearchResults` envelope, unique links, matching titles and snippets, admitted member
reads, and full and conditional denial for the excluded resources. Arguments have a
64 KiB limit; responses have at most 100 hits and 128 KiB. Each probe has 60 seconds.
Credentials stay outside profiles and reports. HTTP or transport failure cannot count
as a successful resource denial. A denied-tool case still requires full and conditional
denial for every ordinary hit; a tool error alone cannot establish access enforcement.

The synthetic checker fixture recreates its source from a temporary document and
qualifies rejection of missing probes, lost state, ineffective mutations, incomplete
acknowledgements, missing readiness, baseline-only notification streams, missing links,
search leakage, readable or enumerated removed members, and conditional authorization bypass.
Creation cases also reject reused identities, missing enumeration, unreadable members,
content in matching conditional responses, and loss of the first member after the second creation.
It establishes checker behavior; it does not qualify production Store recovery.

## Source Checks Through A Gateway

`run_knowledge_source_conformance` executes the same K01–K08 checks for one source
at either its direct endpoint or a gateway. `KnowledgeSourceTarget` requires a checked
HTTP(S) URL, server slug, nonempty owned-scheme set and `KnowledgeRoute`. Credentials
and owner callbacks stay separate. This entry point fails K01 when the endpoint
does not declare the extension.

The gateway route selects collection templates by their declared owner and retains
duplicates for K01 to reject. It converts checked gateway tool names into source-local
names for declaration matching, then builds the gateway name when calling a tool.
Member and collection addresses keep their source-owned schemes. The runner has no
domain registry or implementation dependencies for this selection.

The result uses the shared report format with a `knowledge-{server}` profile ID and
the observed endpoint implementation, which may be the gateway. It contains only
knowledge checks and does not certify the full hosted-server contract. K09/K10
require owner review. The run has a fifteen-minute deadline, ten-second connection
timeouts and 65-second HTTP request timeouts, with redirects disabled. The ordinary
collection and probe limits still apply.

`conformance knowledge-source --url <endpoint> --server <slug> --owned-scheme <scheme>
--route gateway --report <new-path>` runs this checker from the CLI. The route may
also be `direct`; repeat `--owned-scheme` for additional owner schemes. Credentials
use the CLI's existing out-of-band inputs. It writes a private, create-only report
and exits unsuccessfully when a check fails. Sources declaring mutation or search
probes require their owner harness; the CLI supplies no probes and therefore fails
those requirements. Documentation-only sources can use this command directly.

Native fixtures check direct and gateway selection, missing declarations, duplicate
preservation and gateway tool calls for both ordinary and restricted readers. The
fault matrix runs under both naming routes. Installed owner harnesses use the
[shared transport and restart helpers](../../testing/installed/DESIGN.md); their
workload, mutation, fixture admission and cleanup remain owner decisions.

## Authoritative Live-View Profile

A simulation profile may require `list_live_cameras`, `open_live_view`,
`renew_live_view`, and `close_live_view` plus domain-owned camera, product, and
redacted authorization resources. The profile verifies strict schemas, stable per-camera
product identity, actor and browser-instance isolation, token rotation, credential
redaction, App declaration, and authenticated shared-stream admission. Resource URIs retain
the simulation server's own scheme; conformance never requires a shared renderer URI.

The anonymous external simulation fixture exercises this public contract without
claiming visual or GPU acceptance. Hardware RTX rendering, declared tiled-product and
NVENC topology, exact bitstream fanout, frame freshness, and headed-browser
playback remain implementation-owned evidence. The first-party UAV simulation
acceptance supplies that evidence for the NVIDIA runtime.

## Report

Each result carries a stable requirement identifier, status, summary, and bounded
evidence. The report records the negotiated protocol, implementation identity,
advertised capabilities, selected contract revision, and execution interval. A failed
requirement produces a report and a non-zero CLI exit.

Certification reads the live contract declaration and binds it to the selection and
observation. The selected revision must equal the conformance client's supported
revision. The declaration's numeric revision must be the numeric member of that
revision, and its server must match both the expected slug and discovered implementation.
The declaration must mark C18–C21 met. Discover and the MCP lists supply the observed
capabilities. The client follows the relative document links published in `llms.txt`;
it does not synthesize document URLs from parsed identifiers.

## CLI Output

`gateway-token-exchange` accepts `--client-key-file` and `--client-key-id`, also
read from `VEOVEO_SERVICE_CLIENT_PRIVATE_KEY_FILE` and `VEOVEO_SERVICE_CLIENT_KEY_ID`.
The RSA PEM file remains outside repository artifacts and must have owner-only
permissions on Unix. Remote endpoints require HTTPS and an explicit key. The public
conformance key is confined to loopback token endpoints. Installations use distinct
client keys and register their public JWKS through their own configuration.
Token exchanges have a thirty-second deadline and never follow redirects.

Each CLI command reserves standard output for its requested result. Structured resources
therefore remain parseable even when the server emits notifications while the command is
running. Unsolicited progress, task-status, resource-update, and list-change notifications
are operator diagnostics on standard error.
The generation CLI drains request-scoped notifications through the SDK's subscription
handle while waiting for the next Task read. A stream error or premature end fails the
command; it does not submit another Task.

Media generation completion uses the server library's checked result contract. The
CLI compares structured completion metadata with the canonical result resource and
downloads its typed Artifact addresses. Gateway Task handles stay opaque; Media's
result supplies the native Task identity. The CLI requires the current result profile
specified by the [Media design](../../servers/media-mcp/DESIGN.md#catalog-and-result-contracts).

## Distribution

The thin `certify` binary is copied into the digest-addressed
`veoveo/mcp-conformance` OCI image. The image contains no server implementation and
runs as uid 10001. Installation operators mirror it into their private registry or
offline bundle and execute it against extension endpoints.
