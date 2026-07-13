"""Veoveo platform integration for Python MCP servers.

This package gives a hosted Python MCP server the same platform surface its
Rust siblings get from the workspace crates: the gateway internal-trust
verifier, host validation, the durable SurrealDB task runtime, the final MCP
task extension transport, the shared artifact plane, and usage reporting.
"""

__all__ = [
    "artifacts",
    "contract",
    "deployment",
    "host",
    "internal_auth",
    "pagination",
    "task_extension",
    "tasks",
    "telemetry",
]
