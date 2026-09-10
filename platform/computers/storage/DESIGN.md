# Retained Computer Storage

Status: implementation in progress. The runtime has qualified the native filesystem
and registered-writer primitives. This component supplies their durable host boundary;
it is not yet an installed or qualified allocator.

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| `veoveo.io/computer-storage/v1` | Private bounded JSON frames over TLS 1.3 with worker client authentication; exact provider, Computer, template and instance identity |
| Docker volume-plugin API v1 | Named retained volumes, local scope and a private Unix socket; notifications do not transfer writer authority |
| Docker Engine API `1.53` | Exact engine identity and registered-container observation; provider mutations remain with the Computers worker |
| Linux ext4 and loop devices | Fixed, preallocated backing files, `nodev,nosuid`, numeric UID/GID 10001 and a confined `home` subdirectory |
| `veoveo.io/retained-storage-host/v1` and `veoveo.io/retained-home/v1` | Closed local JSON records, atomic publication, explicit incomplete-allocation state and host/engine binding |
| Linux file locks and filesystem durability | One helper owns the metadata root; file and parent-directory synchronization precede success |

## Ownership And Deployment

The Computers worker owns policy, quota admission and the durable lifecycle operation.
This helper owns physical allocation and the admitted writer on one compute host. It
uses a separate process because it needs mount/loop privileges and a stable lifetime
across provider updates. The public gateway, Console and Computer never receive its
Docker socket, private trust material or backing-file paths.

The selected host root is an operator-owned local filesystem directory with private
permissions. Docker and the allocator must observe the same mounted paths. A container
installation requires a dedicated shared mount subtree with bidirectional propagation;
qualification must prove that boundary before activation. The installation must not
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

Changing the admitted instance requires an explicit source-to-target transition. The
helper must prove removal of the exact source resource on the recorded engine and
serialize that proof with admission updates. A late source instance remains denied.
Persist the new binding before admitting its mount. A lost handoff response is resolved
from the durable binding; it never permits reviving the old identity. The native fixture
currently supplies an in-memory transition; it is not evidence for this implementation.

Delete must first remove physical consumers, then unmount and verify loop detachment.
Only an explicitly governed purge may remove retained files. Plugin Remove cannot
perform that purge. Backups require a fenced source and synchronized bytes; a verified
offline block backup is the initial supported mechanism. Local reboot retention is
distinct from host-loss durability. Encryption-at-rest and backup-key ownership remain
installation profile requirements before release acceptance.
