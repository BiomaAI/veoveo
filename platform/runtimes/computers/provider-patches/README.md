# OpenShell Provider Profile

## Standards And Protocols

The private provider uses OpenShell protobuf/gRPC **0.1.2**, based on NVIDIA's
[v0.1.2 release](https://github.com/NVIDIA/OpenShell/releases/tag/v0.1.2), commit
`6648bd0c290efbc41ba131ee9831ee45cd431f94`. The source tree is
`26f142d5c3be825e72f4d1a705ecf6aa5d4d6b10`. Licensed vendored protocol inputs have
independent SHA-256 verification in `../protocol/provenance.json` and
`../protocol/SHA256SUMS`.

Gateway, Docker driver, supervisor and sandbox share the patched profile
`0.1.2-veoveo.1`. The public CLI profile is `0.1.2`. This is a coordinated private
protocol cut; the adapter rejects other gateway and driver versions before mutation.
The installed-user acceptance CLI is the official released executable with its
frozen artifact hash. The provider package also builds an unpatched-source CLI with
Rust 1.99.0; that build has a separate artifact identity.

The Linux/amd64 build uses Rust 1.99.0. Gateway, driver and supervisor use
`x86_64-unknown-linux-gnu`. The injected sandbox uses static
`x86_64-unknown-linux-musl`, following upstream's portable workload boundary.
The supervisor image exposes `/openshell-supervisor`; `/openshell-sandbox` identifies
the injected capability-free boundary. The manifest binds each binary to its source
tree, version, target and image path.

## Patch Graph

The source verifier starts both branches at the licensed upstream tree. It applies
`0.1.2-sandbox-identity.patch` first, followed by the selected branch patch.
`manifest.json` binds each patch's SHA-256, input tree and resulting tree. The gateway
branch adds `0.1.2-docker-admission.patch`. The supervisor branch adds
`0.1.2-replay-boundary.patch`. Exported source trees contain no enclosing Git metadata;
builds materialize the manifest's workspace and lockfile package version.

| Owner | Required behavior and controls |
|---|---|
| Docker driver and sandbox identity | Explicit numeric policy users receive no inherited image-account supplementary groups; private PTYs receive the admitted UID/GID and mode 0600 |
| Sandbox Landlock | O_PATH descriptors classify the same opened inode and permit policy handling of device paths without opening device data |
| Docker driver logging | Workload and companion Docker logs use configured finite rotation; the actual supervisor companion has the configured finite `/var/log` tmpfs |
| Docker driver supervisor restart | Private version-1 guest restoration and generation-owned physical companion binding admit the same running supervisor after gateway-only loss; immutable mismatches and expired/superseded authentication refuse without replacement |
| Docker driver volume admission | Typed NoCopy reaches Engine options with and without subpaths; retained-home initialization cannot precede container registration |
| Gateway mTLS | Exact user common-name allowlist; missing, duplicate and unknown common names cannot become users; guest transport certificates need scoped launch authority |
| Supervisor attachment | A generated ReplayComplete fence belongs to each attachment cursor; history and live bytes remain unmodified; clients enable input only after their fence |
| Upstream launch lifecycle | Fresh launch authentication, credential rotation and generation-correlated start/exit observations replace the selected obsolete scheduling mechanisms only after source and native qualification |

The gateway package runs the existing upstream start-cancellation, durable stop/start,
failed-start reconciliation, launch-auth renewal and stale-exit generation controls.
Docker driver controls cover identity, NoCopy, log bounds, profile image admission and
runtime generations. Focused restart-state controls cover controller-memory loss, exact
selected-template and retained-home restoration, non-overwrite and private permissions,
foreign image/container/labels/mount refusal, missing or corrupt records and credential
exclusion. The driver uses the existing locked Prost codec for this private record. Companion binding controls separately qualify atomic private commit,
exact physical instance and configuration refusal, retained/current JWT admission and
Engine inspection without create/remove/start or credential overwrite. A running
recovery adopts the same companion; explicit stopped-generation Start requires the old
companion's observed absence. The runtime design declares commit ordering, the uncertain
start-before-commit window and mandatory drain before rollback to pre-adoption images.
Startup controls preserve companion candidates and their trust volumes. Running controls
exercise the public Start path with exact recorded-instance admission and a same-ID wait
monitor; archive staging is allowed only during fixture preparation. The binding hashes
verified public authentication facts from the staged bundle snapshot without persisting JWTs.
Stopped-generation controls require companion absence before credential updates, staging
or guest Start, and cover a completed Stop followed by an admitted new generation.
Public Create controls preserve pre-existing binding and trust bytes when a companion
is retained, foreign or uncertain. Private directories, files and symlinks refuse fresh
Create. Cleanup requires the originally claimed directory device/inode and a safe
local refusal before provisioning dispatch. Lost volume Create, guest Create or Start
responses preserve the generation-bound private intent, resources and fencing. Get and
watch retain a separate unknown provisioning condition across process replacement;
Running Start refuses unfinished intent before changing credentials or generation.
Public controls apply an Engine mutation and lose its response, reject cleanup after a
directory inode replacement, and qualify cleanup after a genuine local refusal.
Lifecycle controls exercise name-only and explicit-ID admission, retained intent after
driver replacement, and a paused Stop/Delete racing a Start into unresolved companion
launch. Each request resolves one identity and holds its admission lock through current
intent checks and effects; Stop cannot authorize interruption of a fresh Create.
A safe local failure followed by Stop and corrected Create exercises that separation.
Wait controls distinguish confirmed exits and owned Stop from transport failure, EOF
and structured uncertainty. Unknown observation preserves resources and its monitor
fence; an authenticated session cannot erase the separate unknown condition.

Supervisor controls cover replay and SSH attachments; sandbox
controls cover process identity and Landlock. `control.sh` uses the maintained libtest
runner's multiple OR filters in one invocation per owner target, requires nonzero
passing tests and checks that each selected control ran.

The gateway loader control reads the packaged Host and native TOML produced by their
Rust generators. The owning Rust controls compare the generated bytes with these
package inputs. The source manifest binds each TOML and its generator source with
SHA-256; the build verifies all four inputs before compiling the gateway loader and
strict Docker schema controls. Both generators emit gateway configuration schema 2.
The loader control admits the enabled and absent gateway-owned guest TLS bundle
through effective driver startup validation before effects. It preserves the exact
worker mTLS allowlist and rejects partial bundles, guest paths in a driver table,
plaintext guest TLS, schema 1 and retired Docker network keys. Guest authority denial
still requires the native authenticated transport control. To refresh the inputs, run the existing `generated_host_provider_config` Host control and
`generated_native_provider_config` native-worker control with
`VEOVEO_PROVIDER_CONFIG_EXPORT` pointing to this directory's `generated` subdirectory,
then run both controls without that variable and update the manifest hashes.

The isolated BuildKit compiler runs the two ignored sandbox controls for numeric
supplementary-group clearing and PTY reopening in a separate 300-second invocation.
Only their full test names are selected with `--ignored --exact`. The runner requires
root with effective CHOWN, SETGID, SETUID and SETPCAP capabilities and rejects a
compiler that lacks them. Both named tests must pass. Ordinary process controls
cannot qualify these privileged guarantees; this compiler-only test grants no
installation-host privilege and executes no native provider scenario.

Source controls do not establish retained restart with positive credential TTL, current
run identity, unknown-outcome containment, actual terminal input fencing, or physical
writer settlement. The existing native lifecycle, worker, command, file, maintenance
and host suites must qualify those behaviors against this built profile. Results from
the older provider do not qualify this profile.

## OCI Build

The `computer-provider` build verifies the immutable upstream commit/tree and every
patch transition before exporting source. Its compiler explicitly sets
`RUSTUP_TOOLCHAIN=1.99.0`, checks Cargo and rustc versions, and installs the musl target.
The upstream toolchain file cannot select Rust 1.95. Compiled artifacts use the maintained
BuildKit cache `veoveo-openshell-rust199-bookworm-target` at `/provider-target`; musl
artifacts occupy that cache's target subdirectory. Registry sources keep the existing
compiler-independent cache identity. There is no additional upstream host target.

The build selects the executable-owning `openshell-gateway` package with its
`openshell-gateway/bundled-z3` feature and the Docker driver package. The gateway
feature forwards to `openshell-server/bundled-z3`; the selected server library
controls use that same feature. The package copies only executable paths emitted
by that Cargo invocation and verifies the copied SHA-256 against each emitted
artifact; its receipt records their package identities. Locked
`z3-src` 501.0.0 builds stable Z3 5.1.0 as a static library. CMake >=3.16 and
Python3 come from the selected Bookworm compiler snapshot. `cxx-profile.json`
pins source-built [GCC 16.2.0](https://gcc.gnu.org/releases.html),
[GMP 6.3.0](https://gmplib.org/),
[MPFR 4.2.2](https://www.mpfr.org/mpfr-current/) and
[MPC 1.4.1](https://www.multiprecision.org/mpc/download.html) by source URL and
SHA-512. The GCC pin agrees with its upstream SHA-512 release list. GMP, MPFR
and MPC build as static compiler prerequisites in a separate prefix. GCC enables
C and C++, disables bootstrap and multilib, and builds against glibc 2.36. This
compiler supplies the C++20 `<format>` implementation required by Z3 without
raising the runtime ABI. Building the compiler adds a cached source-build stage;
it does not change the Rust 1.99.0 or static-musl sandbox profiles.

The gateway build sets `CXX` to this compiler and uses locked `z3-sys` 0.13.0's
`CXXSTDLIB=static=stdc++` link hook. Its native search path contains only the
selected `libstdc++.a`. Gateway and driver ELF checks record and print each
executable's DT_NEEDED libraries and maximum required GLIBC symbol version. They
reject dynamic libz3 and libstdc++ dependencies, missing GLIBC version information,
and any required GLIBC symbol version above 2.36. The runtime keeps
its Bookworm libraries for other GNU executables. Ubuntu 24.04's glibc 2.39 can
satisfy the admitted gateway baseline; the actual native executable suites must
still qualify the complete provider. The runtime exports the C++ profile,
compiler version, static archive hash, GCC license and Runtime Library Exception
beside the dependency and symbol-version receipts.

Ops first builds the Dockerfile's `cxx-probe-evidence` target on the maintained
BuildKit builder with a 2400-second outer deadline and a local output directory.
That target compiles and executes a C++20 `std::format` program, checks its
static C++ runtime and GLIBC ceiling, and exports only the receipts. It builds
no OpenShell Rust target and imports no image. Each source archive has a
256 MiB read limit, a 180-second deadline and a SHA-512 check. After the probe
passes and the source checkpoint is committed, Ops runs the maintained complete
`computer-provider` build using the same cached C++ stage and existing provider
Cargo caches. The probe does not qualify Z3, gateway controls, or native provider
behavior; the complete build must pass its solver controls and gateway ELF
checks before native acceptance.

The build checks that the sandbox has neither an ELF interpreter nor a DT_NEEDED
closure and records its file classification. It records the GNU supervisor's dynamic
closure and compiler identities. The runtime image includes the exact source manifest,
patches, upstream license, package inventory and hashes for all five executables.
Its OCI labels bind the patched profile, manifest SHA-256 and supervisor source tree.
The maintained image planner captures the manifest digest from the selected provider
checkout and supplies `PROVIDER_MANIFEST_SHA256` with that build context. The source
stage refuses a missing or mismatched digest before compilation; the final image uses
the same argument for its manifest label. Direct Bake invocations require this admitted
argument and must pass the same source check.
The host image exports both supervisor and sandbox and keeps their hash inventory.

Provider configuration requires a preloaded digest-pinned supervisor image and the
exact injected sandbox binary. The driver inspects the local image before admission;
it does not pull a default supervisor or use a sandbox-image fallback. Missing or
unsupported inputs fail before resource creation. Installed hosts run the companion
with host networking inside the existing private compute-host namespace. The workload
uses `network=none`. The companion drops all capabilities, uses no-new-privileges, runs
with its own UID/GID and read-only root, and receives only read-only channel/state
mounts. It receives no retained home, Docker socket, shared PID namespace or cgroup
namespace.

The template's CPU, memory and configured PIDs ceilings apply separately to each
container. These are maxima, with zero reservations and no pinned CPUs. A Computer
can therefore consume the sum of its workload and companion maxima. The compute
host's finite aggregate ceiling governs pressure; the retained-Computer count quota
is not a memory reservation. The native host suite must inspect both containers'
cgroup ancestry and ceilings beneath that host ceiling.

## Native Artifact Admission

`../tests/native_support/profile.rs` reads a closed
`veoveo.ai/openshell-native-profile/v1` receipt before database, home or daemon
allocation. SHA-256 fields use the shared digest type. Receipt and child-command
reads have allocation bounds; read-only commands have deadlines. The receipt binds
all five executable paths and hashes, the canonical manifest hash, private bridge
gateway IP, and the candidate image's digest, image ID, source tree and supervisor
and sandbox artifact hashes.

Preflight inspects the seeded outer local image and bridge without creating resources.
The image ID and RepoDigest must match the receipt; source labels must match the
manifest and patched supervisor tree. An unpatched official image cannot supply the
candidate companion. All five local binaries must hash and version-match the receipt.
The fixture then preloads that same candidate into its private DinD daemon.

Native gateways bind only the observed private bridge IP and an ephemeral port. Their
certificate includes that IP SAN and uses a unique disposable CA with distinct worker
and guest identities. The installed host uses its private namespace's loopback
endpoint. Native bridge routing does not qualify installed-host routing, cgroups or
mount topology. Standalone runtime cases also own the maintained private daemon;
provider, retained storage helpers, writer enumeration and cleanup use its explicit
socket. No provider fixture can select the installation daemon for its companion.

Gateway relay admission uses the existing fifteen-second session window when the
previous gateway's fresh owner record advertises a local-only endpoint. Current
same-instance, higher-epoch authentication and owner CAS still govern reconnect;
no local-only address is dialed and admission cannot take over another instance.
Server controls exercise the routed relay against the actual Store and session
registry, including no-session deadlines, foreign/stale epochs, principal refusal
and superseded-session selection. Driver launch diagnostics emit only a closed
stage and tonic code while preserving the original unknown outcome and fences.

## Qualification And Maintenance

This package declares a candidate source/build profile. The manifest records
`artifactsIncluded: false` and `executionQualifiedByThisPackage: false`; package source
alone does not provide executable or native qualification. BuildKit compilation,
ELF/version/dependency closure and the full affected native matrix are release gates.
The owning runtime design declares the required drain and checkpoint compatibility.

Veoveo maintains the three patches until upstream provides and qualifies equivalent
identity, replay, NoCopy, logging and mTLS guarantees. Each removal needs the owning
controls and native qualification against the selected stable release. The patch graph
and artifact receipt remain the source of reproducible provenance during that review.
