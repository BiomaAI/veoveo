# Durable Task Runtime Instructions

Follow root AGENTS.md and DESIGN.md. Keep provider-specific state and effects in the
owning domain. Shared recovery must preserve uncertainty and current cancellation
intent. A provider observation claim never authorizes replay of a mutation.

Treat recovery-class/schema changes as deployment contracts. Preserve other qualified
profiles and declare reader compatibility before admitting a new stored class.
Use isolated real-store tests for lease, migration and transaction behavior. Do not
report environment-gated tests as executed when their setup returned early.
