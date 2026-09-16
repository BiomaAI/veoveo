# Workspace Model Execution

## Standards And Protocols

This component exposes typed Veoveo HTTP JSON routes for agent admission, response
runs and cancellation. It uses the repository-qualified Rig revision
`eefffb118579cd4b1a48f8d7cd9bd04354b57634` and its OpenAI-compatible Chat Completions
stream adapter. This is an internal provider adapter, not an OpenAI Responses API
implementation. Existing gateway OAuth, session-family revocation and Work Context
contracts govern the initiating human. MCP `2026-07-28` tool calls and Tasks use the
shared [native operation boundary](../operations/DESIGN.md). A model response and
its Tasks retain separate identities and lifecycles.

## Configuration And Disclosure

`VEOVEO_WORKSPACE_AGENTS` contains a bounded JSON array of typed definitions. Each
record names its installation-local identity, display name, description, provider,
tenant, admitted Work Contexts, instructions, an exact tool allowlist and model configuration. Model settings
contain an HTTP(S) base URL, model name, registered `provider_api_key` secret reference
and output-token budget. Embedded URL credentials, unknown contexts, unregistered
secrets, duplicate identities and unbounded budgets fail startup validation.

The model key is resolved through the existing gateway secret resolver immediately
before dispatch. No key, endpoint, system instructions or execution fence enters a
browser response. A human owner sees provider/model and capability disclosure before admitting the
agent. Existing agent membership records bind the complete definition digest.
Configuration changes require new admission before another run uses that definition.
The default empty catalog permits human collaboration but provides no model fixture.
The Workspace plan records installed model and Task qualification separately from local fixtures.

## Execution

The Rust store owns run identity, claim fencing, fixed context and cancellation as
specified in [`platform/store`](../../../../../../store/src/workspace/runs/DESIGN.md).
`messages.rs` admits the human message and all response intents in one store
transaction. The response does not depend on a second browser request. An owned
background coordinator resolves current definitions and starts independently fenced
workers. Process capacity or definition changes settle unclaimed runs with a closed
failure reason. A human-only turn starts no coordinator. The separate run route
supports an explicit assignment of an existing message authored by that human. Sixteen workers fit in one process; the store separately
enforces concurrent room capacity. Retrying admission preserves the same run ID.

A model call receives a bounded JSON history with stable author IDs and names. Its
triggering human request remains explicit. Older history can be dropped to fit the
64 KiB prompt bound, and the prompt marks that truncation. The trigger itself is never
silently truncated. Model output has a 32 KiB bound. Rig's total call budget is four,
which includes retries and continuations. Content telemetry is disabled explicitly.

The worker batches cumulative text into at most one store publication per second.
Each publication rechecks OAuth session and JWT revocation, current Work Context
membership and the store's run fence. Claim heartbeat, cancellation and output use
the same transaction boundary. The run ends within two minutes or token expiry.
Shutdown drops the provider stream; durable lease recovery later marks interruption.
A browser disconnect does not cancel work and cannot resubmit the model on reconnect.

## Capability Execution

The model receives the intersection of its configured tool allowlist and the
initiating human's current native MCP discovery. Definitions admit at most 64 tools;
their combined model schema is capped at 128 KiB. JSON Schema validators compile
once per run. Arguments are bounded and validated before operation admission.
Every invocation carries the initiating human's bearer in memory. No service
credential or caller-supplied destination participates in execution.

Eight distinct tool operations fit in one response. A UUID derived from the run,
tool name and canonical arguments makes an identical repeated request return the
same private receipt. A new human message can intentionally request another action.
Admission checks the live run fence and current chat authority. The dispatch worker
checks them again after connecting and before sending the native tool call.
Permission failure stops further model turns as well as external dispatch.

The model waits for that bounded invocation receipt before continuing; it does not
wait for the Task to complete. The private journal retains the Task identity across
model completion, navigation and service replacement. Native Tasks input and
cancellation remain explicit human activity actions. Stopping the model prevents
new dispatch; it does not cancel an already accepted Task.

Tool replies sent to the model contain only a receipt explanation. Native results,
input prompts and continuation state never enter shared chat context. This release
does not automatically analyze private tool outputs or chain calls that require
those outputs. Audience-admitted result sharing is a later explicit operation,
as required by the accepted private-to-shared publication boundary.

## Browser Experience And Tasks Boundary

Owners admit agents and choose their response policy through chat details. The
composer displays the current destinations before sending. Explicit selections and
leading `@Name` addresses become typed agent IDs. The server resolves defaults and
automatic participation inside the transaction. Agent output cannot trigger a turn. Each response has its own status and stop control while
humans retain the composer. The bounded activity response contains current agent
membership and the latest 64 runs; stable IDs replace updated output in place.
Human-history pagination remains independent from the active run window.

Stopping a response is distinct from requesting MCP Task cancellation. The accepted
first-class Task behavior is in
[`WORKSPACE_PLAN.md`](../../../../../../../docs/WORKSPACE_PLAN.md#first-class-mcp-tasks).
Task references, native Tasks input rounds, private results and cross-restart
activity have local runtime and browser qualification. Public releases also qualify
real Task completion, cancellation and recovery. Installed native-input acceptance
remains open because the installed domains do not request input.

## Qualification

A Rust HTTP fixture runs the actual Rig streaming client against an explicitly fake
provider and the real disposable database. It exercises atomic two-agent message admission, concurrent replay, two independent streams,
human writing during execution, one cancellation and retry without a second provider
call. A second fixture connects the actual model client to native MCP and the durable
Task runtime. Repeated tool calls create one Task, current discovery narrows the
agent allowlist, private input prompts stay out of model context, and cancelling a
response blocks later dispatch. Browser-edge tests reject caller-selected model settings and forged initiators.
A headed hardware browser fixture checks four authors, response-specific controls,
reconnect and mobile layout. None of these fixtures establishes deployed model or
MCP Task execution. Public evidence and remaining acceptance are recorded in the Workspace plan.

## Reply-Aware Context

Human turns carry a typed human-message or agent-response reference and a bounded
server-captured quote. The store checks same-chat authority and settled response text
inside admission. Each model history turn includes its typed identity; the explicit
request retains its reply reference and quote even when older history is truncated.
These values remain untrusted shared-chat data. Private Task outputs do not enter
this path. The real HTTP model fixture verifies the request quote and excludes the
other chat's text. The persistence design owns migration and coordinated rollout.
