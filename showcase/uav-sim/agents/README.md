# UAV Pilot Agents

Operators publish pilot definitions and deploy managed instances through the Veoveo
API or Console. Each instance uses an isolated generic kernel, its own service
identity and retained memory. The UAV chart installs the reviewed
[template](../deploy/helm/files/agent-template/manifest.json); the lifecycle manager
owns pilot workloads, signing keys and memory claims.

[Instructions](instructions.md) provide seed content for explicit authoring. Published
revisions own the actual instructions, tools, subscriptions and budgets. Editing a
pilot requires no image build, Helm edit or gateway restart.

The requested vehicle is a template parameter. UAV Simulation MCP grants the actual
OAuth principal an exact session, vehicle, permission set and Map mobility profile.
The server enforces that grant again at mission admission and execution. The UAV App
lists eligible managed pilots from current grants in the caller's Work Context.

Pilot memory contains operator intent and canonical resource references. Map MCP owns
named places, routes and mobility profiles. Frames MCP owns world revisions. See the
[design](DESIGN.md) for the managed runtime and migration boundaries.
