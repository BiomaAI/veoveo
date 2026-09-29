# Frames Domain Contract Instructions

Follow the repository instructions and this component's [DESIGN.md](DESIGN.md).
Keep the public spatial model below Frames runtime and Recording consumers. Export it
through the Frames MCP library's isolated contract feature without duplicate types.
Keep domain assertions here and runtime, SQL and authorization checks with the server.
Frame addresses pin a world revision; parsing alone grants no authority or content claim.
