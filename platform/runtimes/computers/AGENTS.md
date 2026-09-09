# Computers Runtime Instructions

Follow root AGENTS.md and this component's DESIGN.md. This is a private provider
adapter, not a hosted MCP server. Domain authority and durable Tasks belong to
Computers; this crate never authorizes a principal or clears a domain operation.

Keep protocol generation local and hash-verified. Preserve the upstream protocol
license and exact provider patch provenance. Generated provider types must not
derive field-by-field Debug that could expose credentials or process output.

Loss of provider observation preserves uncertainty. Never repeat an uncertain
mutation in a reconnect loop. Reconciliation must bind the original provider,
resource and run identity and have a finite budget. Fixtures prove transport and
adapter behavior; they do not establish actual provider containment or retention.

Keep real provider and filesystem faults isolated from installed user Computers.
Retained homes, journals, and referenced templates are not disposable build caches.
