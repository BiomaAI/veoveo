# Required OpenShell provider profile

## Standards And Protocols

The inspected profile uses stock CLI **0.0.116**, based on public OpenShell
[`d1155aa70042d3e2ee49dbfa15346b108b7c1d92`](https://github.com/NVIDIA/OpenShell/tree/d1155aa70042d3e2ee49dbfa15346b108b7c1d92),
with separately qualified gateway and supervisor repairs. These are downstream
patches, not an NVIDIA release or a claim that upstream has accepted them.

The native client currently requires gateway `0.0.117-veoveo.2` and the
Docker driver. The retained terminal requires the matching metadata-capable
supervisor `0.0.117-dev.5+gea0c605`. Stock CLI version and patched provider
versions are deliberately different components of one recorded profile.

## Patch graph

```text
OpenShell v0.0.116 / d1155aa7
  1. positive-TTL retained restart       -> c40054a9
  2. numeric supervisor groups          -> 857b6639
  3. owned terminals / path policy      -> 92d36947
       |-- 4. stale exit-event recheck   -> f03c0f7d
       |     5. bounded logs            -> 32efe0b3
       |     7. retained volume NoCopy  -> 9ccd1611
       |     8. mTLS user admission     -> b9d0916b  [gateway tree]
       `-- 6. explicit replay boundary  -> ea0c605b  [supervisor]
```

The replay patch branches from the common third patch. It is not applied after
the gateway patches to reproduce the recorded supervisor tree. The seventh and
eighth repairs are Veoveo-authored and extend only the gateway branch. Their manifest
entries bind exact base and resulting trees without claiming upstream commits.
Source qualification and native artifact evidence remain separate.

| Repair | Required behavior |
|---|---|
| Positive-TTL retained restart | An authorized Start renews the exact stopped sandbox's expiring bootstrap token atomically; retains its container/home and keeps expiry enforcement |
| Numeric supervisor groups | Run the admitted non-root numeric UID/GID without inheriting unintended supplementary groups |
| Owned terminals / path policy | Correct private PTY ownership and Landlock path-descriptor handling for the real retained terminal |
| Exit-event recheck | An old queued exit must not make a newly started retained run terminal |
| Log bounds | Bound Docker rotation and supervisor log tmpfs without consuming retained-home capacity indefinitely |
| Replay boundary | Preserve raw bytes and deliver generated per-attachment ReplayComplete metadata before enabling terminal input |
| Retained volume NoCopy | Pass typed `no_copy` to Docker's volume options with or without a subpath; retained Computer templates require it to prevent initialization before container registration |
| mTLS user admission | Only explicitly admitted certificate common names become provider users; guest transport certificates still require their scoped sandbox JWT |

## Provenance and review

`manifest.json` records the public base, every patch's dependency, original
source revision/tree, original patch hash and packaged patch hash. Five patches
are unchanged raw diffs. The replay patch's mail envelope was removed to exclude
author contact metadata; its diff payload is unchanged and separately hashed.
This changes the patch-file identity, not its resulting source tree.

The seventh patch's `expectedBaseTree` is the fifth patch's resulting tree. Its
`expectedSourceTree` and packaged SHA-256 identify Veoveo's new source. The six
imported patch bytes and their provenance remain unchanged. Export the new gateway
tree without a parent Git checkout and materialize `gatewayVersion` in workspace
Cargo.toml and local-package Cargo.lock versions; this prevents an enclosing Veoveo
Git revision from becoming the provider version. Keep the supervisor export separate.
The new retained-home fingerprint includes `no_copy: true`. This unreleased profile
rejects the older gateway version; it does not silently run without the mount option.

The eighth patch extends that gateway tree. The earlier native fixture reused its
worker certificate as the guest supervisor transport certificate. The provider's
authentication-only mTLS mode mapped every accepted client certificate to a user
principal. That credential placement is outside the selected installed trust profile.
`[openshell.gateway.mtls_auth]` now includes `user_common_names`, an exact allowlist
with no wildcard or default user identity. An empty or malformed list admits no user.
Missing or ambiguous certificate common names cannot become user identities.

The supported host admits only `veoveo-computers-worker`. Supervisors receive distinct
`veoveo-computer-supervisor` certificates, with private keys owned by root. Their TLS
trust establishes transport; their sandbox JWT supplies scoped provider authority.
The native fixture checks that the guest certificate reaches TLS but cannot invoke
ListSandboxes without a JWT before it creates a Computer. Worker admission and actual
supervisor execution then prove the positive paths. The stock CLI version and
supervisor source tree are unchanged. Provider configuration must deploy with the new
gateway image; earlier native artifact evidence remains a historical checkpoint.

The package verifier applies each patch to a temporary Git index in the graph
above and compares each resulting tree to its recorded expected tree. It does
not build, install, execute or publish the provider. A newly authored commit
after applying a patch has a new commit identity; do not label it as one of the
recorded private commits. Preserve the included Apache-2.0 license and source
notices.

To reproduce the checks, use a disposable Git repository containing the public
base object. For each branch use a separate temporary `GIT_INDEX_FILE`, initialize
it with `git read-tree <base-or-common-tree>`, and run the following for each
patch in that branch:

```sh
git apply --cached --check /absolute/path/to/selected.patch
git apply --cached /absolute/path/to/selected.patch
git write-tree
```

Compare `git write-tree` to that patch's `expectedSourceTree` in the manifest.
The gateway and supervisor indexes both start their branch-specific patches at
common tree `d6c991caf64220519382d93342ef88876a561324`. This procedure modifies
only the scratch index/object store; it needs no provider binaries, running
gateway or private source commit.

## OCI Build

The `computer-provider` Bake target builds the selected gateway, Docker driver and
supervisor from the public base plus this manifest's exact patch graph. It verifies
each patch hash and resulting Git tree before exporting source. The exported trees
have no parent Git checkout; explicit workspace and lockfile versions retain the
qualified component identities. Cargo builds with `--locked` using the profile's
Rust 1.95.0 toolchain, independently of Veoveo's compiler.

The compiler runs the provider's focused mTLS admission tests before building the
gateway. Full native execution and installed trust qualification remain separate from
these configuration predicates.

The compiler uses the official Rust bookworm image pinned in `Dockerfile`. The
runtime uses Debian bookworm-slim index
`sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171`.
Both archive inputs resolve from signed Debian snapshot `20260910T000000Z`.
The runtime closure contains Z3 `4.8.12-3.1` and glibc `2.36-9+deb12u14`.
The provider toolchain remains pinned under CE-07; this package does not upgrade
its protocol or runtime behavior to another upstream release.

The image retains package inventory, binary SHA-256 identities, the source manifest,
patches and the upstream Apache-2.0 license. Source inputs alone enter compilation;
documentation changes reuse compiled layers. A dedicated provider Cargo cache keeps
this dependency graph outside ordinary Console and Veoveo service builds.

Gateway and supervisor compilation are independent stages. Each copies only its own
verified exported source tree from the source stage, while both reuse the compiler
and dependency caches. A gateway-only repair therefore has no supervisor source input
to invalidate. This replaces the former sequential compiler stage that rebuilt the
unchanged supervisor after every gateway repair.

The initial source build and executable version checks pass. Containerized provider
execution and the installed capacity topology still require qualification against
this OCI runtime closure. Earlier host-native artifact evidence is recorded separately.

## Upstream Support

VeoVeo needs a supported source/artifact path for the required behavior: provider
maintainer acceptance, explicitly maintained patches, or a separately selected
and qualified provider release that supplies equivalent semantics. None is
assumed here. The package selects no replacement pin and does not authorize a
runtime upgrade.

Publish a compatibility manifest that binds the gateway, supervisor, CLI,
protocol inputs, driver, architecture, toolchains and test evidence. Reject a
missing replay capability or mismatched provider identity. Do not accept a
version string without the artifact/source binding, disable token expiration,
broaden container privileges, or expose provider credentials to make a profile
appear compatible.

Native builds, policy enforcement, expired-token retained restart, live terminal
behavior and the complete browser/CLI journey must be qualified against that
chosen profile. The included source-application verification does not establish
those execution gates or support for Podman, MicroVM or Kubernetes drivers.
