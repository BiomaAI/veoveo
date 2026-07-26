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
| `veoveo.io/deployment/v2` | named-source installation composition with independent revisions and locks |
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
Secrets, selected revisions, and combined artifact lock. An extension cannot register
itself at runtime or grant itself installation authority.

No supported workflow requires the extension to join the Veoveo Cargo workspace, run
`veoveo-xtask`, edit the Veoveo chart, edit a complete gateway control-plane document,
or share a Git revision with Veoveo.

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
- the extension Helm library artifact;
- optional canonical simulation-runtime profiles;
- the immutable digest and coordinate of every artifact.

The manifest is generated from release inputs. It is not a hand-maintained version
table and never contains credentials.

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
| Integrate | immutable source lock selected by the installation | gateway and deployment composition |

An extension may use Cargo, npm, uv, another build tool, or its own compiled task
runner. Veoveo does not prescribe an external build system.

The Python SDK is the first released language surface. Veoveo builds and verifies its
wheel and source distribution from an exact committed revision with
`cargo xtask release python-sdk`. The canonical Python template pins the exact SDK
version and creates its own lock against an operator-configured private index. Its
Docker build accepts that index only as a BuildKit secret and never copies Veoveo
source into the build context.

## Installation Composition

The extension contributes capabilities. The installation decides exposure and
authorization.

The gateway composer accepts extension-owned server fragments and installation-owned
bindings. It emits one ordinary `GatewayControlPlane`, runs the canonical validator,
and records deterministic provenance. Universal route, mount, MCP path, URI-scheme,
resource-ownership, and policy-identity checks remain in
`GatewayControlPlane::validate`.

Deployment v2 accepts named sources. Each source resolves an independent revision,
image lock, chart, fragment, compatibility manifest, and evidence set. Production
composition consumes immutable artifacts. Local development may use one detached
worktree per explicitly selected source.

## Minimal Platform

An installation selects typed platform components and MCP servers. The component
resolver validates dependencies and prunes disabled workloads, Services, storage,
NetworkPolicies, PodDisruptionBudgets, bootstrap inputs, gateway entries, and digest
requirements together.

The first external profile requires the platform foundation plus Artifact MCP, Frames
MCP, and Recording MCP. This is a reusable component selection rather than a
customer-specific preset.

## Simulation Overlays

Veoveo has one canonical Isaac Sim base lineage. The existing base is upgraded in
place; a parallel runtime image is not introduced. It owns one exact Isaac Sim, Isaac
Lab, Warp, Newton, MuJoCo, Kit/Python, CUDA, driver, and GPU compatibility contract.

An external repository may derive an independently published simulator overlay from
the immutable base digest. The overlay may add domain code, assets, scenarios, and
compatible Kit or Python extensions. It may not silently replace the base runtime
tuple. A conflicting tuple fails compatibility until Veoveo deliberately advances the
base and reruns both first-party and anonymous external-overlay GPU acceptance.

Software rendering and CPU simulation are not acceptance evidence. Runtime images and
charts request the required NVIDIA GPU and fail closed when the hardware path is
unavailable.

## Definition Of Done

The workflow is supported when a clean anonymous external checkout can:

1. Resolve the compatibility manifest and supported SDK from configured private
   artifact sources.
2. Build and test without a path dependency on Veoveo.
3. Run standalone conformance without compiling Veoveo server crates.
4. Publish its own digest-addressed image, chart, gateway fragment, and release
   manifest.
5. Join a deployment v2 installation through an installation-owned binding and source
   lock.
6. Reach the extension through the configured Veoveo origin without editing the
   Veoveo repository.
7. Upgrade or remove the extension without rebuilding the platform.

An optional simulation extension additionally proves that its overlay and the Veoveo
UAV overlay pass GPU acceptance against the same canonical base digest.
