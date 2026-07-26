# Enterprise deployment

Veoveo ships Kubernetes software as OCI images and Helm charts. The installation
owner supplies the cluster, registry access, configuration repository, secrets,
identity, ingress, and reconciliation controller. This boundary keeps a customer
installation recognizable to a Kubernetes platform team and prevents the product
repository from becoming the owner of customer infrastructure.

Helm is the package contract. GitOps is the recommended reconciliation model, with
Argo CD as the maintained reference. An operator may use Flux or direct Helm without
changing the chart, image, configuration, or Secret contracts.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| OCI Distribution Specification | authenticated private image, chart, SBOM, provenance, schema, and evidence distribution |
| Helm and Kubernetes | separately reconciled platform and extension application charts |
| `veoveo.io/deployment/v2` | installation-owned named sources, independent revisions, Bake groups, chart releases, and typed platform selection |
| `veoveo.io/deployment-lock/v2` | immutable source, image manifest, chart content, and resolved-platform record |
| `veoveo.io/gateway-server-fragment/v1` | extension-owned hosted-server contribution |
| `veoveo.io/gateway-binding/v1` | installation-owned exposure, tenant, producer, and authorization policy |
| `veoveo.io/compatibility-manifest/v1` | supported SDK, chart library, standalone tools, schemas, and optional simulation tuple |
| SHA-256 | production image, chart, schema, source-input, and evidence identity |
| OpenID Connect and OAuth 2.0 | installation-owned identity and protected-resource boundary |

## Ownership

| Concern | Owner | Durable source |
|---|---|---|
| Source compilation and image construction | Each source repository | Independent Git revision and repository-local image graph |
| Runtime images | OCI publisher | Registry manifests addressed by digest |
| Platform and extension packages | OCI publisher | Versioned Helm chart artifacts |
| Installation configuration | Installation owner | Private Git repository |
| Credentials and private keys | Installation owner | Secret manager and Kubernetes Secret projections |
| Cluster prerequisites | Installation platform team | Cluster platform repository |
| Application reconciliation | Installation GitOps controller | Declared Applications or equivalent release objects |
| Acceptance evidence | Installation release process | Rust smoke, conformance, and operational evidence |

The build pipeline publishes artifacts. It does not connect to customer clusters.
The configuration repository selects published artifacts. It does not compile
Veoveo. The reconciliation controller reads the desired state and applies it to the
cluster. The smoke harness verifies the resulting installation without owning it.

## Installation Addressing

The installation owner chooses the canonical client-facing origin, for example
`https://veoveo.example.internal`. The name may resolve only through private DNS, an
internal load balancer, or a VPN. It remains distinct from the private OCI registry and
package-index coordinates.

`global.publicBaseUrl`, ingress hosts, OAuth protected resources, redirect URIs, and
gateway issuer metadata derive from that one installation-owned origin. No Veoveo
artifact embeds a universal service hostname, and private deployment does not require a
public Veoveo control plane.

## Release artifacts

One installation release may combine several independently versioned sources.
Production Helm values address images by digest; a mutable tag is not a production
identity. Each source publishes its own images and application chart. The combined
deployment lock records source revisions, image manifest digests, chart-content
digests, and the fully expanded platform selection.

A release pipeline follows this sequence:

1. Resolve the installation's deployment v2 profile from one exact configuration
   revision.
2. Resolve each named source to its independent full revision.
3. Verify that selected Bake targets cover every typed platform and gateway-required
   image, including producer-side RRD transport.
4. Build and push each source-owned image graph with SBOM and provenance attestations.
5. Package and publish each source-owned Helm chart, retaining its registry digest.
6. Write one immutable deployment v2 lock and update the installation repository to
   select it.

The Veoveo source provides a conventional private chart publisher:

~~~bash
REVISION=$(git rev-parse HEAD)
CHART_VERSION=0.1.0-$(git rev-parse --short=12 HEAD)

cargo xtask release helm-charts \
  --registry registry.example.internal/veoveo/charts \
  --version "$CHART_VERSION" \
  --revision "$REVISION"
~~~

An internal development registry may explicitly enable plain HTTP. A fielded registry
uses TLS, authentication, immutable tags, retention policy, and vulnerability scanning
supplied by the installation owner.

The profile release command resolves each source-local Bake graph, moves its persistent
publication worktree to the selected commit without resetting unchanged file metadata,
and publishes without loading images into the host Docker store:

~~~bash
REVISION=$(git rev-parse HEAD)

cargo xtask image builder ensure
cargo xtask release images \
  --profile path/to/deployment.json \
  --profile-revision "$REVISION" \
  --lock-output output/deployment.lock.json
~~~

Direct group publication remains available for source-owned release pipelines. A
Veoveo-backed extension platform uses `external-extension-platform`; the canonical
simulation base and overlays use their dedicated groups. The simulation image group is
an ABI and GPU certification boundary, not proof that Frames, Map, Media, or RRD
services were installed. Deployment profile closure supplies that proof.

The canonical simulation runtime is a separate build dependency because UAV and
external simulator overlays consume it as a named build context. The deployment
profile selects all required groups and records their combined immutable closure.

## Configuration repository

An enterprise configuration repository should contain only installation-owned desired
state:

~~~text
clusters/
  production/
    platform/                 cluster prerequisites and controller configuration
    applications/             root and child reconciliation objects
    values/
      veoveo.yaml             installation identity, capacity, storage, ingress
      extension.yaml          independently deployed domain extension values
      deployment.lock.json    named sources, charts, images, and digests
      images.generated.yaml   Helm digest values projected from the deployment lock
    gateway/
      base.json               installation-owned platform control plane
      bindings/               installation-owned extension exposure and policy
      fragments.lock.json     immutable extension fragment selection
      control-plane.json      deterministic composed output
      public-jwks.json
~~~

Helm values own chart inputs. Kubernetes manifests own resources outside a chart.
The gateway base and bindings own platform exposure and authorization. Extensions own
server fragments in their release artifacts. `gateway-compose` produces the complete
validated control plane and content provenance offline. The GitOps controller may
generate ConfigMaps from committed non-secret outputs. There is no second installation
document that repeats releases, values files, Secret keys, and apply order.

Environment overlays use the native composition mechanism chosen by the enterprise:
Helm values, Kustomize, or the GitOps controller's generator. One setting has one
canonical owner. A value is not copied into a general repository configuration file
merely because one installation needs it.

## Secrets

Charts reference existing Kubernetes Secrets. Secret bytes never enter Helm values,
Git, an Argo CD Application, or a generated ConfigMap. An enterprise may project those
Secrets with External Secrets Operator, Secrets Store CSI Driver, Sealed Secrets, or
its established platform mechanism.

The platform chart expects these Secret contracts by default:

| Secret | Required keys |
|---|---|
| veoveo-surreal-admin | username, password |
| veoveo-surreal-runtime | username, password |
| veoveo-installation-secrets | internal-signing-key-der-b64, internal-signing-key-id, internal-trust-jwks, oidc-client-secret, authorization-server-private-key-der-b64, refresh-delivery-key-b64, console-session-key, object-store-access-key, object-store-secret-key, media-provider-api-key, google-maps-api-key, media-provider-webhook-secret, simulation-view-renderer-control-token, simulation-view-pose-control-token |
| simulation-view-pose-tls | tls.crt.der, tls.key.der, ca.crt.der |

An extension declares its own least-privilege Secret references. It does not add
provider credentials to the platform Secret merely for convenience. Registry
credentials use a Kubernetes image pull Secret selected through Helm values.

Argo CD repository credentials are also platform Secrets. They authorize Argo to read
the enterprise Git and OCI repositories; they are not application credentials.

## Controller boundary

The enterprise owns the GitOps controller. Veoveo applications must not install,
upgrade, configure, or delete that controller. A local reference environment may
bootstrap a pinned Argo CD version as a platform fixture, but the root Veoveo
Application begins only after the controller and its repository credentials exist.

A root application may create the installation namespace, non-secret ConfigMaps,
ingress connectors, an AppProject, and child Applications. The platform chart is one
child. Each optional private MCP extension is another child with its own chart version,
values, health, rollback, and lifecycle.

The controller reconciles drift continuously. Routine releases change Git and let the
controller converge. kubectl apply and helm upgrade are bootstrap and recovery tools,
not concurrent owners of the same application resources.

## Independently deployed MCP extensions

An extension packages its Kubernetes workload in its own Helm chart. The installation
adds a child application for that chart, selects its immutable release manifest, and
binds its gateway fragment through installation-owned policy. The deterministic
composer registers routes and capabilities in the complete control plane. This
separates scheduling and rollout while preserving one MCP authority and one
authorization boundary.

An extension application normally selects two sources:

- the immutable OCI chart version;
- the installation Git repository containing values, bindings, and the deployment lock.

Private MCP servers follow the same pattern. They use their repository's native build
system and do not join the Veoveo workspace. Their chart consumes the versioned
`veoveo-extension` library from the configured private OCI registry or verified offline
bundle. Their gateway fragment still uses the canonical typed control-plane model,
internal assertion trust, policy checks, audit path, and URI identities. The complete
normative server requirements, including the well-known docs and contract resources,
are in
[`mcp/contract/DESIGN.md`](../mcp/contract/DESIGN.md).

## Direct Helm

Argo CD is not a runtime dependency of Veoveo. An enterprise with another release
controller can render or install the same packages directly:

~~~bash
helm upgrade --install veoveo   oci://registry.example.com/veoveo/charts/veoveo   --version "$CHART_VERSION"   --namespace veoveo   --create-namespace   --values values/veoveo.yaml   --values values/images.generated.yaml   --wait
~~~

The operator must apply the gateway ConfigMap and provision every referenced Secret
before Helm starts workloads. Another GitOps system should express those same ordering
and ownership boundaries rather than translating them into a Veoveo-specific
orchestrator.

## Upgrade and rollback

A release change updates chart versions and image digests in one reviewed commit.
Automated reconciliation may self-heal configuration drift, but promotion between
environments remains an explicit Git change. Rollback restores the previous known-good
versions and locks. Database migration compatibility belongs to release notes and must
be evaluated before promotion.

A production gate checks controller health, application sync, pod readiness, persistent
storage, ingress, OAuth discovery, MCP capability discovery, and required GPU capacity.
Domain acceptance then exercises the installed workload through its public contract.
