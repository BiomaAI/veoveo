# Required OpenShell provider profile

## Standards And Protocols

The inspected profile uses stock CLI **0.0.116**, based on public OpenShell
[`d1155aa70042d3e2ee49dbfa15346b108b7c1d92`](https://github.com/NVIDIA/OpenShell/tree/d1155aa70042d3e2ee49dbfa15346b108b7c1d92),
with separately qualified gateway and supervisor repairs. These are downstream
patches, not an NVIDIA release or a claim that upstream has accepted them.

The native client currently requires gateway `0.0.117-veoveo.1` and the
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
       |     7. retained volume NoCopy  -> 9ccd1611  [gateway tree]
       `-- 6. explicit replay boundary  -> ea0c605b  [supervisor]
```

The replay patch branches from the common third patch. It is not applied after
the two gateway patches to reproduce the recorded supervisor tree. The seventh
repair is Veoveo-authored and extends only the gateway branch. Its manifest entry
binds the exact base and resulting trees without claiming an upstream commit.
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

## Upstream decision needed

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
