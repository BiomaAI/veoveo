# Agent Kernel

## Standards And Protocols

The kernel uses MCP `2026-07-28` through the pinned RMCP/Rig adapters. Gateway
authentication uses OAuth client credentials with RS256 private-key assertions.
Model completion uses the approved Chat Completions adapter, without claiming
every provider API. JSON manifests, managed generation admission and the dispatch
preflight endpoint are repository-owned contracts. SurrealDB owns scheduling and
Task delivery; DuckDB and Rerun hold local analytical projections.

## Managed Configuration

An installation-reviewed manifest defines the runtime package and memory schema.
The lifecycle manager supplies its fixed identity, generation and model connection.
At startup the kernel resolves the current managed registration and immutable
revision. It checks that identity and model match, then overlays authored
instructions, subscriptions and execution limits after manifest environment
expansion. Authored text never becomes an environment template.

Each episode retains its generation and dispatch epoch. The kernel checks current
gateway authority before model and tool calls, including its local tools. Stop and
disable close further dispatch. Pause allows the bounded current episode to drain
and retains Task observation without admitting new model work. Task completion
continues through the existing durable watcher and wake path.

The process publishes managed readiness after its gateway connection and tools are
installed. It relinquishes its scheduler lease before a generation handoff. The
lease remains the sole writer boundary for retained memory and episode execution.

Active episodes observe their durable dispatch fence through outbox LIVE events.
A five-second recovery read covers missed events. Stop drops the active runner
without cancelling an already accepted domain Task. A whole-episode deadline also
bounds model and tool work. DuckDB represents a stopped projection as an error
with an explicit stop reason; the canonical runtime state remains `stopped`.
