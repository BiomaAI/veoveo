# Computer Host Node Runtime

The reference Computer Host selects `veoveo-computer-host`. Its container cgroup
bounds all descendants to eight CPUs, 12 GiB and 4096 PIDs. Kubernetes supplies
CPU and memory limits from the chart. This node-owned containerd handler supplies
the PID limit. The Host verifies finite limits before starting its internal daemon.
Stock OpenShell supervisors share this aggregate budget. Supported workload
settings keep their upstream semantics.

Kubernetes has no PodSpec PID-limit field. Kubelet `podPidsLimit` applies to all
Pods on a node and does not configure this container's private cgroup root. The
reference uses an opt-in RuntimeClass instead. Other Pods keep their existing
runtime selection, including NVIDIA handlers.

## Supported Inputs

[The reference node](../../k3d.yaml) uses K3s `v1.37.0-k3s1` with containerd
`v2.3.4-k3s1`. [The extension template](config-v3.toml.tmpl) invokes K3s's stock
`base` template before adding the handler. It preserves generated defaults and
NVIDIA handlers. [The RuntimeClass](runtime-class.yaml) selects stock
`io.containerd.runc.v2` with `SystemdCgroup=false`, matching the reference node's
existing runc handler. It introduces no runtime executable or provider patch.
An installation with an existing custom template must merge this single handler
into that template and preserve its other entries.

The pinned [stock OCI generator](https://github.com/k3s-io/containerd/blob/v2.3.4-k3s1/cmd/ctr/commands/oci/oci.go)
produces the full default specification. Its [CRI resource adapter](https://github.com/k3s-io/containerd/blob/v2.3.4-k3s1/internal/cri/opts/spec_linux_opts.go)
applies CPU and memory fields without replacing the base PID limit. Actual
container cgroup inspection must still qualify that result after node activation.

The complete OCI base specification comes from the installed stock generator,
not a reduced JSON document. During separately authorized node maintenance, run
these preparation commands **on each eligible node**, using an isolated staging
directory. They generate configuration without installing it or restarting K3s:

```sh
mkdir -m 700 computer-host-runtime-staging
cd computer-host-runtime-staging
k3s ctr version
k3s ctr oci spec --platform linux/amd64 > stock-oci.json
jq '.linux.resources.pids.limit = 4096' stock-oci.json > computer-host-oci.json
jq -e '.process != null and .root != null and (.mounts | length > 0) and (.linux.namespaces | length > 0) and .linux.resources.pids.limit == 4096' computer-host-oci.json
jq -e --slurpfile stock stock-oci.json '. == ($stock[0] | .linux.resources.pids.limit = 4096)' computer-host-oci.json
sha256sum stock-oci.json computer-host-oci.json
```

Require the recorded containerd version to match the supported input. Keep the
full generated process, root, namespaces, mounts and security defaults. The
comparison proves that the PID value is the sole transformation. No credentials
belong in these files.

## Activation And Qualification

Node configuration installation and new-Pod qualification are open. The chart
and these files alone do not activate or qualify the handler. Ops owns the
maintenance window, backups, node changes and rollback. Keep Knowledge and
Embedding available under the installation's accepted maintenance constraints;
a K3s restart on the single reference node needs explicit coordination.

After configuration review, Ops installs the generated `computer-host-oci.json`
and extension `config-v3.toml.tmpl` under
`/var/lib/rancher/k3s/agent/etc/containerd/`, preserving any existing template.
Containerd loads base specifications at startup. Restart K3s only within the
approved window, then inspect its rendered config: existing default and NVIDIA
handler definitions must agree with the saved configuration, and the only new
handler must point at the generated file. Apply `runtime-class.yaml` separately
before admitting a Host Pod. No other workload should select this class.

Drain Computers with unresolved-operation and retained-writer fences intact.
Create a new Host Pod using the matching image, configuration and chart; restarting
an old container does not qualify a new base specification. Confirm its private
cgroup root reports finite CPU and memory limits and `pids.max=4096`, and require
the Host startup checks to pass. Qualify stock workload/supervisor descendants
under that aggregate budget and retained-home lifecycle controls before activation.

For rollback, stop admitting Computers and drain the Host before restoring the
saved node template and restarting K3s. Remove the RuntimeClass only after no Pod
selects it. Preserve retained PVCs and provider operation outcomes throughout.

The supported mechanisms are documented by [K3s containerd configuration](https://docs.k3s.io/advanced#configuring-containerd),
[containerd CRI runtime configuration](https://github.com/containerd/containerd/blob/v2.3.4/docs/cri/config.md),
and [Kubernetes RuntimeClass](https://kubernetes.io/docs/concepts/containers/runtime-class/).
