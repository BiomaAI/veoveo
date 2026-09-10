# Console MCP Resource Sharing

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| MCP `2026-07-28` | Stateless Streamable HTTP, Discover and request-scoped `subscriptions/listen` through the existing pinned RMCP client |
| Veoveo Console session | One cached MCP client per authenticated scope and source-token fingerprint |
| MCP resource notifications | Exact acknowledged resource filter, shared upstream listener and bounded local invalidations |
| Server-sent events | Existing App feed and native Computer feed; neither feed carries provider credentials |

The Console's MCP pool serves Apps and native Computer observers. Resource capacity
is shared within an authenticated client. The existing installation resource limits
continue to bound that aggregate; this change adds no configuration aliases or pins.
Catalog projection stays in `mcp_client.rs`. Resource registration, reference counts
and listener cleanup live in `resources.rs`.

Subscriptions for the same URI share one upstream request. Each downstream UUID owns
one reference, and reusing that UUID cannot increase its count. The final reference
cancels the upstream request with bounded acknowledgment. Registration reserves capacity
before opening a listener. If registration is cancelled while awaiting its reply, a
drop guard releases the exact pending identity. The upstream SDK independently cancels
its pending wire request. Partial acknowledgments fail admission and return capacity.

Unexpected source completion invalidates this cached client's resource observers.
Loss is monotonic for that client. Existing downstream feeds close, and a subsequent
pool acquisition replaces the client even when its OAuth token remains valid. Ordinary
unsubscribe does not invalidate other observers. Auth-token replacement and shutdown
close the old client's observers. This currently retires the complete auth-scoped
resource client on unexpected loss; per-resource recovery remains a possible refinement
when measurement justifies its additional ownership state.

Tests use actual RMCP HTTP subscriptions to qualify sharing, partial admission,
capacity return, source loss and recovery, and explicit cancellation. The fixture
keeps its source open until cancellation or deliberate termination. It waits for remote
cleanup separately from local transport acknowledgment.
