# External Extension Contract

This document defines the supported boundary between a Veoveo installation and an
independently owned extension repository. An extension keeps its source, build graph,
tests, image, chart, release cadence, and domain behavior outside the Veoveo repository.
It joins an installation through versioned artifacts and installation-owned composition.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| Model Context Protocol | the hosted-server profile in `mcp/contract/DESIGN.md` |
| JSON Schema 2020-12 | generated schemas for controlled extension and installation inputs |
| OCI Distribution Specification | authenticated image, chart, schema, conformance, SBOM, and provenance distribution |
| Helm and Kubernetes | separately reconciled application charts sharing one installation identity and policy boundary |
| `veoveo.io/compatibility-manifest/v1` | exact tested platform, SDK, chart, conformance, and optional simulation-runtime tuple |
| `veoveo.io/extension-release/v1` | immutable extension source, artifact, fragment, chart, conformance, and optional runtime-overlay declaration |
| `veoveo.io/gateway-server-fragment/v1` | extension-owned hosted-server declaration |
| `veoveo.io/gateway-binding/v1` | installation-owned exposure and authorization declaration |
| `veoveo.io/deployment/v6` | optional repository-development profile with installation-owned values, exact platform publication, managed GPU allocator closure, and configurable private-registry transport |
| `veoveo.io/deployment-lock/v6` | immutable installation revision, source evidence, and managed allocator artifacts emitted by that repository-development publication flow |
| `veoveo.io/simulation-runtime-build-lock/v1` | exact canonical Isaac simulation-base inputs and GPU requirements |
| `veoveo.io/simulation-conformance-result/v1` | hardware result for one immutable simulator overlay and base |
| `veoveo.io/simulation-runtime-release-evidence/v1` | paired first-party and anonymous-overlay evidence published through private OCI |
| SHA-256 | artifact identity and composition input integrity |

The repository pins the exact compatible tool and dependency versions when each
artifact is implemented. Pre-release dependencies require a recorded product reason
and an exact source revision.

## Ownership

Veoveo owns platform contracts, supported SDKs, the extension Helm library,
conformance tooling, gateway and deployment composition, and first-party runtime
bases. The extension owns its domain implementation, source revision, build system,
container image, application chart, gateway server fragment, domain smoke tests, and
release manifest.

The installation owns its client-facing origin, registry and package-source
coordinates, trust roots, credentials, tenant bindings, authorization policies,
Secrets, selected release manifests, and digest-pinned desired state. An extension
cannot register itself at runtime or grant itself installation authority.

No supported workflow requires the extension to join the Veoveo Cargo workspace, run
`veoveo-xtask`, edit the Veoveo chart, edit a complete gateway control-plane document,
or share a Git revision with Veoveo.

The coding-agent procedure is
[`EXTERNAL_REPOSITORY_INTEGRATION.md`](EXTERNAL_REPOSITORY_INTEGRATION.md). It uses the
published contracts and ordinary repository or GitOps tools. A Veoveo-specific
installation coordinator is not part of the external contract.

## Private Distribution

Published means versioned, immutable, and retrievable from an installation-configured
artifact source. It does not mean anonymously accessible or internet-public.

A connected installation may use an authenticated customer-operated registry or a
Veoveo-operated private registry. A disconnected installation imports the same
artifacts from a verified offline bundle. Registry hostnames, repository prefixes,
credential Secret references, and trust roots remain installation configuration.

The client-facing origin is independent from artifact distribution. An installation
origin may resolve only through private DNS, an internal load balancer, or a VPN.
Images and charts may resolve from another internal hostname. Neither hostname is a
Veoveo-wide constant.

Production images, schema bundles, and conformance artifacts use immutable SHA-256
identities. Helm dependencies carry a lock digest. Mutable tags may aid local
development and are not production release identities.

## Compatibility Manifest

`veoveo.io/compatibility-manifest/v1` identifies one tested Veoveo release surface. It
contains:

- the compatibility release identity and platform version;
- every supported contract kind and version;
- the exact SDK artifacts and supported language-runtime ranges;
- the standalone conformance artifact;
- the deterministic gateway composer artifact;
- the extension Helm library artifact;
- optional canonical simulation-runtime profiles;
- the immutable digest and coordinate of every artifact.

The manifest is generated from release inputs. It is not a hand-maintained version
table and never contains credentials.

After the Python, Helm, and extension-support image releases have produced immutable
evidence, generate the compatibility bundle from the same source revision:

```sh
cargo xtask release compatibility \
  --revision "$REVISION" \
  --release 0.1.0 \
  --platform-version 0.1.0 \
  --python-evidence output/releases/python-sdk/"$REVISION"/release-evidence.json \
  --python-artifact-base python://packages.internal.example/veoveo \
  --helm-evidence output/releases/helm/"$REVISION"/0.1.0/release-evidence.json \
  --image-evidence output/releases/images/"$REVISION"/extension-support.release-evidence.json \
  --simulation-evidence output/releases/simulation-runtime/"$REVISION"/0.1.0/release-evidence.json
```

The command rejects mixed revisions, a Helm library without an OCI publication, and
image evidence without both standalone tools. It emits the compatibility manifest,
every controlled external JSON Schema, and release evidence containing SHA-256 hashes
for the inputs and outputs.

The Rust types and schema generator live in `extensions/contract`. The generated
schema is the machine interface; consumers do not need the Rust crate.

## Extension Release Manifest

`veoveo.io/extension-release/v1` describes one independently published extension
release. It contains:

- a stable extension identifier and semantic version;
- an exact source revision;
- the required Veoveo compatibility release;
- every extension-owned immutable artifact;
- one Helm application chart;
- one gateway server fragment;
- one or more conformance results;
- an optional canonical simulation-base profile and digest.

The release manifest does not contain installation bindings, Secret values, tenant
identities, policies, or a complete gateway document. Those facts belong to the
installation.

## Build, Test, Smoke, Package, Integrate

| Activity | Extension repository | Veoveo boundary |
|---|---|---|
| Build | native language and repository-local image graph | supported SDK and schemas |
| Test | unit, integration, schema, and policy tests | compatibility manifest |
| Smoke | domain lifecycle scenarios | standalone conformance artifact |
| Package | extension image, application chart, fragment, and release manifest | extension Helm library and artifact schemas |
| Integrate | extension release and digest-pinned chart values selected by the installation | gateway composition and typed platform requirements |

An extension may use Cargo, npm, uv, another build tool, or its own compiled task
runner. Veoveo does not prescribe an external build system.

The Python SDK is the first released language surface. Veoveo builds and verifies its
wheel and source distribution from an exact committed revision with
`cargo xtask release python-sdk`. The canonical Python template pins the exact SDK
version and creates its own lock against an operator-configured private index. Its
Docker build accepts that index only as a BuildKit secret and never copies Veoveo
source into the build context.

`mcp/conformance` is the standalone protocol boundary. Its typed profile supplies
extension-owned names and URI schemes at runtime; the binary has no dependency on a
domain server crate or central server registry. `conformance certify --profile
<file> --report <file>` records implementation identity, negotiated protocol,
advertised capabilities, applicable requirement results, and bounded evidence. The
same binary ships in the private `veoveo/mcp-conformance` OCI artifact.
Hosted certification always checks the well-known MCP resources and the authenticated
same-origin administrative docs projection. The gateway internal bearer is supplied
out of band and never serialized into the profile or report.

`deploy/helm/veoveo-extension` is the private library-chart source for
`veoveo.io/extension-helm-library/v1`. It exports stable installation and component
labels, production image resolution, restricted security contexts, platform
environment, HTTP probes, bootstrap mounts, the recording forwarder, and declared
network policy. `cargo xtask release helm-charts --revision <commit> --version
<version>` packages the library with the application charts from a clean exact
revision and writes SHA-256 release evidence. Supplying `--registry
<private-host/repository>` publishes them through the authenticated Helm OCI client
and records each returned manifest digest.

## Installation Composition

The extension contributes capabilities. The installation decides exposure and
authorization.

The gateway composer accepts extension-owned server fragments and installation-owned
bindings. It emits one ordinary `GatewayControlPlane`, runs the canonical validator,
and records deterministic provenance. Universal route, mount, MCP path, URI-scheme,
resource-ownership, and policy-identity checks remain in
`GatewayControlPlane::validate`.

The standalone command is:

```sh
gateway-compose \
  --base installation.gateway.json \
  --fragment extension.gateway-fragment.json \
  --binding installation.gateway-binding.json \
  --output gateway.json \
  --requirements gateway-requirements.json \
  --provenance gateway-provenance.json
```

`mcp/composer` builds the native `gateway-compose` executable and the private
`veoveo/gateway-composer` OCI image. The anonymous documents in
`extensions/examples` prove the same workflow without a source dependency or a
customer identity.

Deployment v4 remains an optional repository-development facility. Its profile may
reside in an installation repository, where the installation owns registry transport,
Helm overrides, and the exact profile revision. It derives the minimal platform target
set, accepts independently versioned workload and extension sources, and rejects
missing, unnecessary, duplicate, or colliding image identities before publication.
`cargo xtask release images --profile` emits a combined lock for that
source-publication workflow.

A fielded installation does not clone those sources or run Veoveo `xtask`. Its coding
agent verifies the published compatibility and extension-release manifests, pins image
and chart digests in the installation's ordinary Helm or GitOps inputs, composes the
gateway, renders the releases, and commits the desired state.

## Minimal Platform

An installation selects typed platform components and MCP servers. The component
resolver validates dependencies and prunes disabled workloads, Services, storage,
NetworkPolicies, PodDisruptionBudgets, bootstrap inputs, gateway entries, and digest
requirements together.

The `extension-foundation` preset contains the platform foundation plus Artifact MCP,
Frames MCP, and Recording MCP. The chart owns each first-party workload definition;
installations select typed component and server names instead of reproducing
Deployments. Custom selections can add Map, Media, and Optimization.

Gateway requirements are evaluated before Helm. `artifact`, `frames`, `map`, `media`,
and `optimization` each require the corresponding MCP server. Optimization also
requires the GPU cuOpt executor. `recording` and `rrd` require the Recording MCP and
hub. Artifact audiences declared by composition must appear in the installation's
admitted audience set.

The same resolution produces the required Veoveo image closure. An RRD capability adds
the producer-side `recording-forwarder`; Recording adds `recording-hub` and
`recording-mcp`. Optimization adds `optimization-mcp` and `cuopt-executor`. Profile
validation and publication derive the exact platform targets and fail before a build or
push when an image is missing or unnecessary. `external-extension-platform` remains a
convenient direct-build group for Artifact, Frames, Map, Media, Recording, and RRD
transport. Profiles do not use it as a second source of selection truth. None of those
services is copied into an extension image.

The anonymous installation fixture exercises this closure without a customer identity:

```sh
cargo xtask smoke profile-validate \
  --profile testing/fixtures/external-extension-installation/deployment.json
```

The contract acceptance creates independent platform, extension, and installation Git
repositories and produces one validated combined development lock:

```sh
cargo test -p veoveo-deploy-contract --test multi_repository
```

## Simulation Overlays

Veoveo has one canonical Isaac Sim base lineage. The existing base is upgraded in
place; a parallel runtime image is not introduced. It owns one exact Isaac Sim, Isaac
Lab, Warp, Newton, MuJoCo, Kit/Python, CUDA, driver, and GPU compatibility contract.

The `isaac-sim-6` profile currently fixes Isaac Sim
`6.0.1-rc.7+release.42383.32955d8d.gl`, Isaac Lab `v3.0.0-beta2.patch1` at
`ffff603eafc6b74264a5261cc0183d6a65390d78`, Warp `1.15.0`, Newton `1.4.0`,
MuJoCo `3.10.0`, MuJoCo Warp `3.10.0.3`, Python `3.12.13`, CUDA `12.9`, and
Kit `110.1.2`. `platform/runtimes/simulation/simulation-runtime.lock.json` also pins the
upstream image, source archive, and wheel digests.

An external repository may derive an independently published simulator overlay from
the immutable base digest. The overlay may add domain code, assets, scenarios, and
compatible Kit or Python extensions. Its Dockerfile extends the inherited runtime
environment, for example `ENV PYTHONPATH=/opt/extension:${PYTHONPATH}`. Replacing
`PYTHONPATH` is unsupported because it removes platform and Isaac Lab roots. A
conflicting tuple or a non-monotonic Python path fails certification until Veoveo
deliberately advances the base and reruns both first-party and anonymous
external-overlay GPU acceptance.

Software rendering and CPU simulation are not acceptance evidence. Runtime images and
charts request the required NVIDIA GPU and fail closed when the hardware path is
unavailable.

Build the two overlay candidates without conflating them with the platform services:

```sh
cargo xtask image build --group showcase-uav-sim-overlay-acceptance
```

Release builds must attach SBOM and provenance attestations. Certify their immutable
registry identities separately:

```sh
cargo xtask smoke simulation-certify \
  --deployment-lock "$DEPLOYMENT_LOCK" \
  --base-image "$REGISTRY/veoveo/simulation-runtime@$BASE_DIGEST" \
  --overlay-image "$REGISTRY/veoveo/uav-sim-runtime@$UAV_DIGEST" \
  --overlay-kind first-party-uav \
  --source-revision "$REVISION" \
  --output output/simulation-certification/first-party-uav.result.json

cargo xtask smoke simulation-certify \
  --deployment-lock "$DEPLOYMENT_LOCK" \
  --base-image "$REGISTRY/veoveo/simulation-runtime@$BASE_DIGEST" \
  --overlay-image "$REGISTRY/veoveo/simulation-overlay-acceptance@$EXTERNAL_DIGEST" \
  --overlay-kind anonymous-external \
  --source-revision "$REVISION" \
  --output output/simulation-certification/anonymous-external.result.json
```

Both results must name the same base digest, source revision, build-lock digest, and
component tuple. Publication then creates the private OCI evidence bundle:

```sh
cargo xtask release simulation-runtime \
  --revision "$REVISION" \
  --version 0.1.0 \
  --deployment-lock "$DEPLOYMENT_LOCK" \
  --first-party-result output/simulation-certification/first-party-uav.result.json \
  --anonymous-result output/simulation-certification/anonymous-external.result.json
```

The deployment lock is also the registry transport authority. TLS remains the default.
An internal HTTP registry works only when the lock explicitly selects
`insecure-http`; changing the image reference to a loopback alias is rejected because
it changes the recorded OCI identity. Each certification keeps a sibling
`*.transcript.log`, including partial output from a failed or timed-out GPU launch.
The first exact-digest run materializes the overlay into the local Docker certification
cache. Later runs verify its source label and reuse it. Operators can reclaim those
large local images with `cargo xtask image certification-cache-prune --confirm
veoveo-simulation-certify-cache`.

## Definition Of Done

The workflow is supported when a clean anonymous external checkout can:

1. Resolve the compatibility manifest and supported SDK from configured private
   artifact sources.
2. Build and test without a path dependency on Veoveo.
3. Run standalone conformance without compiling Veoveo server crates.
4. Publish its own digest-addressed image, chart, gateway fragment, and release
   manifest.
5. Join an installation through an installation-owned binding, selected extension
   release, and digest-pinned chart values.
6. Reach the extension through the configured Veoveo origin without editing the
   Veoveo repository.
7. Upgrade or remove the extension without rebuilding the platform.

An optional simulation extension additionally proves that its overlay and the Veoveo
UAV overlay pass GPU acceptance against the same canonical base digest.
