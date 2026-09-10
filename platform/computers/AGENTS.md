# Computers Instructions

Follow the root instructions and the component DESIGN.md. Keep domain state and
authority independent of the provider transport. Only a Computers worker may invoke
the private runtime; a Console or MCP projection calls this domain.

Retained Computers are a collection. Human and service ownership use canonical
identity and Work Context. Admission, operation fences and audit/outbox changes
must commit atomically. An uncertain provider effect never releases a fence.

Use isolated real-store tests for concurrent admission and durable transitions.
Never point destructive fixtures at the installation database or retained homes.
