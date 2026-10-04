# Workspace Module Instructions

Follow the repository [agent instructions](../../AGENTS.md) and
[Workspace module design](DESIGN.md). This crate owns the Workspace persistence
module declaration; `apps/workspace` owns the browser product.

Keep `schema` independent of contract and runtime dependencies. Persistence migrations
and queries belong together under this owner when they move from Store. Preserve
participant admission and transactional import semantics across that move.
