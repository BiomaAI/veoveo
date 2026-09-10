# Retained Storage Instructions

Follow root AGENTS.md and this component's DESIGN.md. This component is the
privileged storage-host boundary, not the Computers domain or an MCP server.
Reuse the runtime's storage identity and registered-writer matcher. Do not invoke
OpenShell lifecycle or authorize a user from this helper.

Keep filesystem mutation, durable metadata, Docker observation, and authenticated
transport in focused modules. A missing reply, marker or mount never permits
formatting an existing backing file. Unmount notifications and lease expiry cannot
admit a different writer. Use isolated filesystem and daemon fixtures for faults.

The storage client trust root is dedicated to authorized Computers workers. It must
not admit provider guest certificates. No host path or user-supplied volume option
crosses the private allocation protocol. Retained files and journals survive helper
rollouts and routine build-cache cleanup.
