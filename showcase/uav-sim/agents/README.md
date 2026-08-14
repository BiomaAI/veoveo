# UAV Pilot Agents

The showcase runs one isolated generic agent-kernel process per configured vehicle. All
instances use the reviewed `deploy/helm/files/agents/pilot/manifest.json`; Helm supplies a distinct agent id,
OAuth client, signing key, data volume, and requested vehicle id to each process.

The requested vehicle id is not an authorization shortcut. UAV Simulation MCP grants
the authenticated OAuth principal an exact session, vehicle, permission set, and Map
mobility profile. The server enforces that grant again at admission and execution.

Pilot memory contains operator intent and canonical resource references. Named places,
coordinates, routes, restrictions, and mobility profiles stay in Map MCP. World and
frame revisions stay in Frames MCP. This directory packages a UAV use case without
adding UAV concepts to the agent kernel, gateway, Console, or conversation contract.
