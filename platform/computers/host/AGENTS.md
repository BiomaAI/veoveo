# Compute Host Instructions

Follow root AGENTS.md and this component's DESIGN.md. This launcher owns only the
private compute container's processes and runtime configuration. The Computers
worker owns lifecycle decisions and storage owns retained writer admission.

Never connect to a host Docker socket, run in the host network namespace, expose
provider credentials to guests, or prune retained data during restart. Keep child
process deadlines and ordered cleanup explicit. Qualification uses isolated owned
containers and volumes; preserve unrelated Docker and Kubernetes workloads.
