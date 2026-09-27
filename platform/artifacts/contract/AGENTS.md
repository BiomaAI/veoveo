# Artifact Plane Contract Instructions

Follow the root instructions and this component's [DESIGN.md](DESIGN.md).
Keep occurrence identity and metadata independent of MCP, HTTP, database, provider,
and asynchronous runtime dependencies. Authentication and access decisions belong
to their existing owners. Preserve public representations when moving definitions;
qualify explicit wire changes and URI admission rules through the owning tests.
