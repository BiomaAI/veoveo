# Retained Computer Storage

Status: the journal, filesystem backend, private mTLS service and Docker volume plugin
pass isolated native qualification, including shared mounts across helper replacement.
Durable physical handoff also passes its native fault case. Worker maintenance
orchestration, release packaging and installed acceptance remain implementation work.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| `veoveo.io/computer-storage/v1` | Private bounded JSON frames over TLS 1.3 with worker client authentication; exact provider, Computer, template and instance identity |
| Docker volume-plugin API v1 | Named retained volumes, local scope and a private Unix socket; notifications do not transfer writer authority |
| Docker Engine API `1.53` | Exact engine identity and registered-container observation; provider mutations remain with the Computers worker |
| Linux ext4 and loop devices | Fixed, preallocated backing files, `nodev,nosuid`, numeric UID/GID 10001 and a confined `home` subdirectory |
| `veoveo.io/retained-storage-host/v1` and `veoveo.io/retained-home/v1` | Closed local JSON records, atomic publication, explicit incomplete-allocation state and host/engine binding |
| Linux file locks and filesystem durability | One helper owns the metadata root; file and parent-directory synchronization precede success |
| util-linux and e2fsprogs command profiles | Bounded `fallocate`, `mkfs.ext4`, `blkid`, `losetup` and `findmnt` calls from the pinned Computer-image environment; selected outputs are parsed explicitly |

## Ownership And Deployment

The `computer-storage` Bake target uses the shared Veoveo Rust compiler and a
digest-pinned Debian trixie runtime. Signed archive snapshot `20260910T000000Z`
fixes e2fsprogs, util-linux and their package closure; the image retains its package
inventory. The image runs the storage executable as root at the trusted compute-host
boundary. Image construction alone does not qualify a container's mount propagation,
loop devices or retained restart. The selected installed topology must prove them.

The Computers worker owns policy, quota admission and the durable lifecycle operation.
This helper owns physical allocation and the admitted writer on one compute host. It
uses a separate process because it needs mount/loop privileges and a stable lifetime
across provider updates. The public gateway, Console and Computer never receive its
Docker socket, private trust material or backing-file paths.

The selected host root is an operator-owned local filesystem directory with private
permissions. Docker and the allocator must observe the same mounted paths. A container
installation publishes the allocator's mounts through a dedicated `rshared` bind;
the daemon receives them through its corresponding `rslave` bind. Qualification must
prove that boundary before activation. The installation must not
change mount propagation for unrelated host paths or restart the host Docker daemon to
install the helper. Packaging and installed qualification remain delivery work.

The storage client CA is distinct from the provider guest CA. Its certificates admit
only the Computers worker. Docker's plugin socket is local to the compute host and
private to the daemon/helper. General Docker administration and host root are trusted
operator powers; this profile does not establish VM-grade tenant isolation.

## Durable Allocation

Each metadata root binds one provider UUID, Docker engine UUID and provider namespace.
Opening it with different identities fails. A file lock excludes another helper; it
does not stand in for physical writer exclusion. The owner never deletes the lock file
while the root exists. Records use Computer UUID filenames and deny unknown fields.

Prepare first persists an allocating record. It may create a new backing file exactly
once, then preallocate and format it, mount the filesystem, establish the admitted home
permissions and persist Ready with the backing-file identity. A crash leaves explicit
incomplete state. Repeating Prepare cannot reformat that state or silently seed a new
home. Files from incomplete allocation continue to count against physical capacity.

Restore requires the recorded provider, Computer, template and admitted instance. It
verifies the backing file and filesystem identity before reopening the mount. It does
not resize, replace or format a file. A restart may reattach the exact verified backing
file; missing or substituted bytes require recovery. Free-space reserve applies before
allocation, including concurrently admitted homes, logs and temporary host usage.

An allocation record is outside the user's mounted `home` subdirectory. File publication
uses a private temporary file, a synchronized file and an atomic directory operation,
followed by parent synchronization. A failed acknowledgement preserves uncertainty;
subsequent reads determine which complete record is present.

## Writer Handoff And Retention

A mount uses the runtime's exact registered-writer matcher. It requires the recorded
engine and one registered container with the current namespace and complete binding.
Stopped containers still reserve ownership. Mount/Unmount retries and nested `docker
cp` never decrement a counter that grants another writer. Producer mounts require
Docker NoCopy, as qualified by the selected provider patch.

The first successful mount persists the exact Docker container ID and provider
resource ID before returning its path. A later container that copies the same labels
cannot acquire that writer's admission. The qualified provider's Stop/Start retains
its Docker container and creates a new process inside it. Physical container removal
requires explicit maintenance, rather than silently claiming the old instance again.

Changing the admitted instance requires an explicit source-to-target transition. The
helper must prove removal of the exact source resource on the recorded engine and
serialize that proof with admission updates. A late source instance remains denied.
Persist the new binding before admitting its mount. A lost handoff response is resolved
from the durable binding; it never permits reviving the old identity.

Handoff carries the durable operation UUID, source/target instance and template, and
the recorded source provider resource ID. The helper requires an exact 404 for its
recorded Docker container, zero registered volume consumers and unchanged engine
identity. It synchronizes the filesystem, normally unmounts it, detaches its loop and
verifies that no association remains. Linux may acknowledge detach while autoclear
waits for another namespace's mount; that state requires recovery and cannot admit
the target. A failed attempt retains the recorded source. A later bounded request
may resume after authoritative physical observation; it never reformats a home.

Each target instance has an immutable transition file outside the user's filesystem.
That file is synchronized before the home record admits the target and clears its
physical claim. The target is then restored and may acquire its own exact container.
The same operation can resolve a lost reply while that target remains current,
including after helper restart. A different operation cannot claim that transition,
and a stale operation cannot restore a superseded instance. Reusing a previous
instance as a new target is rejected before physical mutation. A rollback uses a
fresh instance with the selected old template, after fencing the current writer.
Template changes must preserve the recorded capacity; resizing remains unsupported.

The current handoff requires a previously claimed source writer. A maintenance
target that never mounted needs an explicit abandoned-admission recovery path in
worker maintenance integration; this helper does not infer a provider outcome from
the absence of a physical claim. The unreleased v1 home record now requires its
writer state. No installed record or rolling compatibility profile is admitted yet.

Delete must first remove physical consumers, then unmount and verify loop detachment.
Only an explicitly governed purge may remove retained files. Plugin Remove cannot
perform that purge. Backups require a fenced source and synchronized bytes; a verified
offline block backup is the initial supported mechanism. Local reboot retention is
distinct from host-loss durability. Encryption-at-rest and backup-key ownership remain
installation profile requirements before release acceptance.

## Private Service And Docker Plugin

`veoveo-computer-storage --config <json-file>` runs both private endpoints.
The closed configuration binds host identity, persistent root, free-space reserve,
admitted template fingerprints/capacities, exact Docker Unix socket, plugin name/socket,
listen address and dedicated worker TLS files. It accepts no ambient Docker endpoint.
TLS permits version 1.3 with required worker certificates. Trust inputs cannot be group
or world writable; the private key is restricted to its process owner. The plugin
socket resides in a root-owned 0700 directory and has mode 0600. The journal lock must
be held before replacing a stale socket; a live listener is never unlinked.

The worker transport admits at most sixteen concurrent connections. TLS handshake and
frame reads each have a five-second deadline; an entire request has 180 seconds. Each
connection carries one request and reply, bounded at 1024 bytes. The service parses
the concrete generated IDL types from the original bytes, retaining duplicate-key
rejection. Successful replies echo the request identity and configured capacity.
Readiness requires the configured provider/template and current engine identity.

The Docker client uses API 1.53 over its explicit Unix socket, bounded replies and
five-second request deadlines. It disables redirects, proxies and automatic retries.
Volume creation first reads the exact canonical name and refuses another driver or
caller options. A missing response cannot authorize formatting. Preparing/restoring
the filesystem releases its mutex before Docker volume creation, because that call
can synchronously invoke the plugin's Create/Get methods.

The plugin accepts the selected Docker volume API shape, including null or empty
Create options. Create confirms an admitted Ready allocation and cannot allocate one.
Mount observes the recorded engine and all registered consumers, including stopped
containers, before restoring the exact admitted home. The same filesystem mutex
serializes that check with storage mutations. The plugin has 32 concurrent call slots,
4096-byte requests and a 180-second operation deadline, separate from worker slots.
Unmount preserves authority, and Remove requires governed deletion. Metadata listing
is bounded at 4096 allocations and never reads Computer file contents.

`tests/native_service.rs` launches the actual binary in a disposable privileged
container, using the pinned Computer image and a separate isolated Docker 29.8 daemon.
The maintained runtime daemon fixture now has an explicit mount-propagation choice.
An independently signed guest certificate and a foreign provider cannot use the
allocator. The production client prepares a 512 MiB home twice without replacement.
A registered Computer writes it, uses nested Docker copy, survives helper removal
while continuing to write, and reopens the same files after helper restart and
Stop/Start. An unadmitted replacement fails both during contention and after the
original resource is removed. The fixture removes its containers before unmounting,
verifies loop detachment and removes only its own retained data and trust material.
The same fixture exercises durable handoff. It holds a separate private mount after
source removal, proves the failed transfer leaves that writer physically active, then
releases it and completes the transfer with the original bytes. It drops the first
handoff response, retries the exact operation, restarts the helper, rejects a late old
instance as the sole consumer, and transfers to a new template without reusing an
instance identity. The worker's native fixture reuses this launcher with the admitted
template, production allocation client and native provider on that isolated daemon.
Public installed acceptance and worker maintenance retain their separate requirements.

## Filesystem Backend And Native Evidence

`filesystem` requires root, a persistent ext-family host volume and a free-space
reserve. The native profile uses ext4; volatile, overlay and remote roots are rejected.
A new reservation creates its
backing file exclusively, preallocates its complete size and formats ext4 with the
Computer UUID. An existing incomplete reservation returns Recovery Required. Ready
restoration verifies the recorded file device/inode/length, ext4 type and UUID, exact
loop mapping, mount target/options and home UID/GID. New homes start with mode 0700.
Owners may change permissions inside their Computer, including on the home directory;
ordinary restore preserves those choices. The surrounding host metadata and allocation
directories remain private to the helper. Missing or changed identities
cannot trigger formatting. Helper exit does not purge an allocation or release a writer.

`command` owns the finite filesystem-tool calls. Arguments are separate process
arguments, the environment is fixed, output is bounded and an expired operation kills
its child. Command output is never a public failure payload. Kernel mount operations
use the qualified Nix crate. No new dependency version is selected for this backend.

`tests/native_filesystem.rs` runs the production backend as root in two disposable
containers using the existing pinned Computer image. It preallocates 512 MiB, reaches
ENOSPC, preserves a file and reopens the allocation after the original helper container
exits, including an owner's changed home permissions. It rejects another instance and a changed ext4 UUID without changing the stored
identity. A separate cleanup phase verifies loop detachment before removing fixture
files. The fixture uses an explicit local Docker socket, isolated mount namespaces and
no network. It does not qualify bidirectional mount propagation, Docker plugin access,
writer handoff or host-loss durability.

Journal process tests run outside concurrent fixture forks. An inherited descriptor can
retain a file lock until exec closes it, as specified by
[`std::fs::File`](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock).
Production continues to report Busy while another descriptor retains ownership.
