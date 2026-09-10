# Computer Transport Instructions

Follow root instructions and this component's DESIGN.md. This is a shared gateway/BFF
transport library. It cannot authorize a principal, select a provider or renew a grant.
Keep tokens out of Debug, URLs and diagnostics. Use the owning gateway/Console TLS
builder; the transport client disables redirects and selects HTTP/1.1. Relays enforce only deadlines
received from their authenticated upstream; no local timer can manufacture renewal.
Test expiry during blocked reads and writes, epoch/sequence failures, bounded buffers
and untrusted input. This crate must not depend on the domain store or provider SDK.
