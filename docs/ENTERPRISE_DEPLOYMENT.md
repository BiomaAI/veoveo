# Enterprise deployment

Veoveo ships Kubernetes software as OCI images and Helm charts. The installation
owner supplies the cluster, registry access, configuration repository, secrets,
identity, ingress, and reconciliation controller. A Kubernetes platform team will
recognize the result, and the Veoveo repository never owns customer infrastructure.

Helm is the package contract. GitOps is the recommended reconciliation model, with
Flux as the maintained reference. An operator may use another controller or direct Helm without
changing the chart, image, configuration, or Secret contracts.

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| OCI Distribution Specification | authenticated private image, chart, SBOM, provenance, schema, and evidence distribution |
| Helm and Kubernetes | separately reconciled platform and workload application charts |
| Flux 2.9.5 / GitOps Toolkit | maintained reference using `source.toolkit.fluxcd.io/v1`, `kustomize.toolkit.fluxcd.io/v1`, and `helm.toolkit.fluxcd.io/v2`; other controllers consume the same Helm and configuration contract |
| `veoveo.io/deployment/v8` | optional repository-development publication profile with exact platform selection, installation-owned Helm values, and managed GPU allocator closure |
| `veoveo.io/deployment-lock/v8` | immutable installation, source, and managed allocator evidence from the repository-development publication flow |
| SHA-256 | production image, chart, schema, source-input, and evidence identity |
| OpenID Connect and OAuth 2.0 | installation-owned identity and protected-resource boundary |

## Ownership

| Concern | Owner | Durable source |
|---|---|---|
| Source compilation and image construction | The Veoveo fork | Git revision and repository-local image graph |
| Runtime images | OCI publisher | Registry manifests addressed by digest |
| Platform and workload packages | OCI publisher | Versioned Helm chart artifacts |
| Installation configuration | Installation owner | Private Git repository |
| Credentials and private keys | Installation owner | Secret manager and Kubernetes Secret projections |
| Cluster prerequisites | Installation platform team | Cluster platform repository |
| Application reconciliation | Installation GitOps controller | Declared source, Kustomization, and release objects |
| Acceptance evidence | Installation release process | Rust smoke, conformance, and operational evidence |

The [Autonomy Harness](AUTONOMY_HARNESS.md) divides responsibility for always-on agents
between Veoveo and the installation, and lists the tests that show each agent's effects
stay within its authority. Helm readiness reports workload health. The gateway also
probes each hosted server's health endpoint, so a degraded server shows up in the
Console.

The build pipeline publishes artifacts. It does not connect to customer clusters.
The configuration repository selects published artifacts. It does not compile
Veoveo. The reconciliation controller reads the desired state and applies it to the
cluster. The smoke harness verifies the resulting installation without owning it.

## Installation Addressing

The installation owner chooses the client-facing origin, for example
`https://veoveo.example.internal`. The name may resolve only through private DNS, an
internal load balancer, or a VPN. It is separate from the private OCI registry and
package-index addresses.

`global.publicBaseUrl`, ingress hosts, OAuth protected resources, redirect URIs, and
gateway issuer metadata derive from that one installation-owned origin. No Veoveo
artifact embeds a universal service hostname, and private deployment does not require a
public Veoveo control plane.

Artifact downloads, Console downloads, and public-share links also use that
origin. RustFS or an external S3-compatible service is private installation
infrastructure. It has no client-facing ingress, DNS requirement, or presigned delivery
contract.

## Release artifacts

An installation release selects independently deployable components built from its
Veoveo fork. Production Helm values address images by digest; unchanged components
can retain images built at earlier revisions. The [fork workflow](FORK_DEVELOPMENT.md)
keeps code and upstream integration in the repository.

1. Test the selected fork revision and the components it affects.
2. Publish the required images and application charts.
3. Pin their digests in installation values and GitOps release objects.
4. Update the complete gateway control plane and its installation-owned policies.
5. Render each selected chart and validate its required services and Secrets.
6. Commit desired state for reconciliation and verify installed behavior.

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

Veoveo's profile publisher is for development and source-publication acceptance. The profile may live in a separate installation repository without making
`xtask` part of the fielded runtime or GitOps contract. It is documented in
[`LOCAL_DEPLOYMENT_PROFILES.md`](LOCAL_DEPLOYMENT_PROFILES.md) and is not required in an
installation repository.

Veoveo release pipelines can also publish image groups directly. A partial domain platform uses `domain-platform`; the simulation base and
overlays use their dedicated groups. The simulation image group certifies ABI and GPU
compatibility. It does not show that Frames, Map, Media, Optimization, or RRD services
were installed; the installation's typed chart selection and gateway requirements show that.

The simulation runtime is a separate build dependency because UAV and
simulator overlays consume it as a named build context. The deployment
profile derives the exact required platform targets and records their combined
immutable closure.

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
      workload.yaml           independently deployed domain workload values
      images.yaml             reviewed image repositories and manifest digests
    gateway/
      control-plane.json      complete gateway registration and policy
      public-jwks.json
~~~

Helm values own chart inputs. Kubernetes manifests own resources outside a chart.
The complete gateway configuration owns server registration, exposure and
authorization. The GitOps controller generates ConfigMaps from committed non-secret
inputs. There is no second installation document repeating releases, values files,
Secret keys and apply order.

Environment overlays use the native composition mechanism chosen by the enterprise:
Helm values, Kustomize, or the GitOps controller's generator. Each setting has one
owner. Do not copy a value into a shared repository configuration file because one
installation needs it.

The Console's public OAuth identity and its network route are configured separately. Keep `consoleBff.oauthResource` at the public protected-resource URL and set
`consoleBff.mcpTransportUrl` to the endpoint reachable by the BFF pod. Corporate roots
belong in a non-secret installation ConfigMap selected by
`consoleBff.outboundCa.existingConfigMap`; the chart mounts its configured PEM key and
the BFF adds those roots to the standard verifier. A deployment/v8 source lists the
owning values file under the platform release's `installationValues`. Missing ConfigMap
data blocks the pod mount, while malformed trust material blocks BFF startup.

## Secrets

Charts reference existing Kubernetes Secrets. Secret bytes never enter Helm values,
Git, a Flux Kustomization, or a generated ConfigMap. An enterprise may project those
Secrets with External Secrets Operator, Secrets Store CSI Driver, Sealed Secrets, or
its established platform mechanism.

The platform chart expects these Secret contracts by default:

| Secret | Required keys |
|---|---|
| veoveo-surreal-admin | username, password |
| veoveo-surreal-runtime | username, password |
| veoveo-installation-secrets | internal-signing-key-der-b64, internal-signing-key-id, internal-trust-jwks, oidc-client-secret, authorization-server-private-key-der-b64, refresh-delivery-key-b64, console-session-key, recording-playback-token-key, object-store-access-key, object-store-secret-key, media-provider-api-key, google-maps-api-key, media-provider-webhook-secret |

A workload declares its own least-privilege Secret references. Its provider
credentials stay out of the platform Secret. Registry
credentials use a Kubernetes image pull Secret selected through Helm values.

Flux repository credentials are also platform Secrets. They authorize Flux to read
the enterprise Git and OCI repositories; they are not application credentials.

`recording-playback-token-key` is independent base64 text that decodes to exactly
32 random bytes. It signs only recording-scoped Redap read tokens and must not reuse a
gateway, refresh-delivery, Console session, object-store, or provider key.

## Controller boundary

The enterprise owns the GitOps controller. Veoveo applications must not install,
upgrade, configure, or delete that controller. A local reference environment may
bootstrap a pinned Flux version as a platform fixture, but the root Veoveo
Kustomization begins only after the controller and its repository credentials exist.

A root Kustomization may create the installation namespace, non-secret ConfigMaps,
ingress connectors, OCI sources, and HelmReleases. The platform chart is one release.
Each separately deployed MCP workload is another release with its own chart version,
values, health, rollback, and lifecycle.

A HelmRelease selects immutable input objects. Its OCIRepository name includes the
complete selected chart manifest digest. Generated Helm values ConfigMaps have content
suffixes and `immutable: true`; Kustomize rewrites `spec.valuesFrom[].name` through its
configured name references. A release-input commit changes the chart reference and the
values references on the same HelmRelease object. Flux can then wait for the new chart
source instead of combining its previous artifact with new values.

The pattern matters when a chart changes its values schema. Updating a stable values
ConfigMap and a stable OCIRepository separately can trigger an upgrade before the new
chart artifact is available. The Bioma reference exercises the immutable-input pattern
for Veoveo and its UAV workload. Other installation repositories use the same pattern.
[Flux documents generated values references](https://fluxcd.io/flux/guides/helmreleases/#refer-to-values-in-configmaps-generated-with-kustomize).

The controller reconciles drift continuously. Routine releases change Git and let the
controller converge. Use kubectl apply and helm upgrade only for bootstrap and recovery, never to manage
application resources alongside the controller.

## Independently Deployed MCP Workloads

A fork can package a domain workload in its own Helm chart. The installation pins
its chart and image, declares required Secrets and registers the server in the typed
gateway control plane. Separate releases preserve independent rollout and failure
isolation. MCP requests still pass through installation authentication, grants and audit.

Every upstream declares an MCP endpoint and a required non-MCP `health_url`. The
gateway treats only a successful health response as healthy. It does not infer
health from an MCP authorization failure or method rejection.

Fork charts use the internal `deploy/helm/common` helpers through a local dependency
and publish the resulting application chart. Remote MCP integrations use their
supported transport and authentication contracts. The hosted-server requirements are
in [`mcp/contract/DESIGN.md`](../mcp/contract/DESIGN.md).

## Direct Helm

Flux is not a runtime dependency of Veoveo. An enterprise with another release
controller can render or install the same packages directly:

~~~bash
helm upgrade --install veoveo \
  oci://registry.example.com/veoveo/charts/veoveo \
  --version "$CHART_VERSION" \
  --namespace veoveo \
  --create-namespace \
  --values values/veoveo.yaml \
  --values values/images.yaml \
  --wait
~~~

The operator must apply the gateway ConfigMap and provision every referenced Secret
before Helm starts workloads. `values/veoveo.yaml` supplies the explicit
`gateway.controlPlaneRevision` digest for the complete mounted public bundle. Another
GitOps system should enforce the same ordering and ownership with its own
mechanisms. Veoveo does not ship an orchestrator for it.

## Upgrade and rollback

A release change updates selected release manifests, chart versions, and image digests
in one reviewed commit.
Automated reconciliation may self-heal configuration drift, but promotion between
environments is always an explicit Git change. Rollback restores the previous known-good
manifests and digests. Database migration compatibility belongs to release notes and
must be evaluated before promotion.

A production gate checks controller health, application sync, pod readiness, persistent
storage, ingress, OAuth discovery, MCP capability discovery, hosted-server health
endpoints, and required GPU capacity.
Domain acceptance then exercises the installed workload through its public contract.
