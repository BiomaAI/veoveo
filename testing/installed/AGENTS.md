# Installed Harness Instructions

Follow the root instructions and [the harness design](DESIGN.md).

- Keep domain assertions and mutations in the source owner's test. Shared helpers
  handle transport, installation selection, lifecycle operations and report I/O.
- Require explicit installation inputs and private credential files. Native fixtures
  must never acquire installation credentials or invoke these installed helpers.
- Check fixture ownership and initial state before mutation, then reconcile owned
  effects even when dispatch or later checks fail. Do not remove pre-existing grants.
- Restart only a declared workload whose component selector matches the owner.
  Use native Kubernetes watches, finite deadlines and one mutation dispatch.
- Keep source-conformance results separate from GPU, visual and retrieval acceptance.
