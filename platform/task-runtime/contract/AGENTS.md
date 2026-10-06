# Task Contract Instructions

Follow root instructions and this component's DESIGN.md. Keep lifecycle and recovery
values independent of execution services and storage. The `surreal` feature owns the
explicit SDK adapter; default consumers must not enable it. Preserve spelling,
declaration order, native literal kinds and optional NONE behavior. Recovery semantics
and domain payloads stay with their existing owners.
