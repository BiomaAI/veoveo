# Agent Kernel

## Standards And Protocols

The kernel uses MCP `2026-07-28` through the pinned RMCP/Rig adapters. Gateway
authentication uses OAuth client credentials with RS256 private-key assertions.
Model completion uses the approved Chat Completions adapter, without claiming
every provider API. JSON manifests, managed generation admission and the dispatch
preflight endpoint are repository-owned contracts. SurrealDB owns scheduling and
Task delivery; DuckDB and Rerun hold local analytical projections.
Resource reads negotiate `ai.veoveo/knowledge-source` through the shared extension
contract and validate its SHA-256 content binding.

## Resource Context

The gateway connection declares the knowledge extension through Rig's client
capabilities. Resource reads retain its credential preflight and connection rotation.
The read adapter checks an observation against the requested URI and original text,
then keeps that observation beside the text and a single provenance line. The line
names the collection, revision, modification time when known, and observation time.
An unsolicited not-modified result fails because the kernel holds no revision cache.

The existing 64 KiB item, 128 KiB response and 512 KiB episode limits count source
text, serialized observation bytes and the provenance line. Validation never truncates
source text or changes the bytes that its digest describes. Read counts, wall time,
family admission and paging limits apply to knowledge and ordinary resources alike.

## Analytical Memory

The kernel opens local memory through `veoveo-duckdb-runtime` and imports SQL quoting
from `veoveo-duckdb-mcp` with defaults disabled and only `contract` enabled. This shares
the DuckDB owner's syntax handling without enabling its hosted process or service clients.
Memory queries retain the runtime's file, network and configuration restrictions.

## Managed Configuration

An installation-reviewed manifest defines the runtime package and memory schema.
The lifecycle manager supplies its fixed identity, generation and model connection.
At startup the kernel resolves the current managed registration and immutable
revision. It checks that identity and model match, then overlays authored
instructions, subscriptions and execution limits after manifest environment
expansion. Authored text never becomes an environment template.

Each episode retains its generation and dispatch epoch. Dispatch also requires the
current scheduler lease owner and fence, including replacement within one generation. The kernel checks current
gateway authority before model and tool calls, including its local tools. Stop and
disable close further dispatch. Pause allows the bounded current episode to drain
and retains Task observation without admitting new model work. Task completion
continues through the existing durable watcher and wake path.

The kernel commits episode admission before entering the execution continuation
that constructs model and tool work. Failed admission leaves that continuation
uninvoked. A native database test exercises a stale scheduler fence and checks both
the rejected transaction and the successful admission order.

The process publishes managed readiness after its gateway connection and tools are
installed. It relinquishes its scheduler lease before a generation handoff. The
lease remains the sole writer boundary for retained memory and episode execution.

The existing scheduler maintenance tick checks credential age while idle. At the
configured refresh fraction it mints a replacement connection, restores resource
listeners and publishes the new epoch to Task watchers. This maintenance admits no
episode and makes no model call. Each attempt is bounded to six seconds; failed
renewal leaves request dispatch subject to the same fail-closed preflight.

Active episodes observe their dispatch fence through native table feeds, replaying
commits after reconnect and rechecking current authority at the known lease expiry.
A failed source closes dispatch. Stop drops the active runner
without cancelling an already accepted domain Task. A whole-episode deadline also
bounds model and tool work. DuckDB represents a stopped projection as an error
with an explicit stop reason; the canonical runtime state remains `stopped`.
