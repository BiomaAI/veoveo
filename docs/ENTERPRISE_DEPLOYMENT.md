# Enterprise deployment

Veoveo ships Kubernetes software as OCI images and Helm charts. The installation
owner supplies the cluster, registry access, configuration repository, secrets,
identity, ingress, and reconciliation controller. This boundary keeps a customer
installation recognizable to a Kubernetes platform team and prevents the product
repository from becoming the owner of customer infrastructure.

Helm is the package contract. GitOps is the recommended reconciliation model, with
Argo CD as the maintained reference. An operator may use Flux or direct Helm without
changing the chart, image, configuration, or Secret contracts.

## Ownership

| Concern | Owner | Durable source |
|---|---|---|
| Source compilation and image construction | Veoveo release pipeline | Git revision and Docker Bake definitions |
| Runtime images | OCI publisher | Registry manifests addressed by digest |
| Platform and extension packages | OCI publisher | Versioned Helm chart artifacts |
| Installation configuration | Enterprise | Private Git repository |
| Credentials and private keys | Enterprise | Secret manager and Kubernetes Secret projections |
| Cluster prerequisites | Enterprise platform team | Cluster platform repository |
| Application reconciliation | Enterprise GitOps controller | Declared Applications or equivalent release objects |
| Acceptance evidence | Enterprise release process | Rust smoke output and operational evidence |

The build pipeline publishes artifacts. It does not connect to customer clusters.
The configuration repository selects published artifacts. It does not compile
Veoveo. The reconciliation controller reads the desired state and applies it to the
cluster. The smoke harness verifies the resulting installation without owning it.

## Release artifacts

Every release publishes all first-party images under one source revision. Production
Helm values address those images by digest through global.imageDigests; a mutable
tag is not a production identity. The platform chart and each independently deployed
extension chart receive a version containing the release revision and are pushed to
an OCI chart repository whose tags are immutable.

A release pipeline follows this sequence:

1. Build and push the selected Docker Bake groups directly to the OCI registry.
2. Resolve each published image manifest digest and write the installation image lock.
3. Package the platform and extension Helm charts with the committed source revision
   as appVersion.
4. Push the chart packages to the OCI chart repository and retain their registry
   digests in release evidence.
5. Update an installation configuration repository to select the chart versions and
   image lock.

The repository provides a conventional chart publisher:

~~~bash
REVISION=$(git rev-parse HEAD)
CHART_VERSION=0.1.0-$(git rev-parse --short=12 HEAD)

just charts-publish registry.example.com/veoveo/charts   "$CHART_VERSION" "$REVISION"
~~~

The command accepts plain_http=true only for the loopback k3d registry. A fielded
registry uses TLS, authentication, immutable tags, retention policy, and vulnerability
scanning supplied by the installation owner.

Docker Bake publishes images without loading them into the host Docker store:

~~~bash
REVISION=$(git rev-parse HEAD)
export VEOVEO_REGISTRY=registry.example.com/veoveo
export VEOVEO_IMAGE_TAG="$REVISION"

docker buildx bake platform-full --push
docker buildx bake showcase-uav-sim-base --push
docker buildx bake showcase-uav-sim --push
~~~

The UAV base is a separate publication phase because the runtime consumes it as a
named build context. Independent targets within a phase remain eligible for concurrent
BuildKit execution.

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
      images.lock.yaml        image repository and digest selection
    gateway/
      control-plane.json      MCP catalog, OAuth clients, policy, and routing
      public-jwks.json
~~~

Helm values own chart inputs. Kubernetes manifests own resources outside a chart.
The gateway control-plane document owns the MCP catalog and authorization policy.
The GitOps controller may generate ConfigMaps from committed non-secret files.
There is no second installation document that repeats releases, values files, Secret
keys, and apply order.

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
| veoveo-installation-secrets | internal-signing-key-der-b64, internal-signing-key-id, internal-trust-jwks, oidc-client-secret, authorization-server-private-key-der-b64, refresh-delivery-key-b64, console-session-key, object-store-access-key, object-store-secret-key, media-provider-api-key, google-maps-api-key, media-provider-webhook-secret |

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
child. Each optional or customer-authored MCP extension is another child with its own
chart version, values, health, rollback, and lifecycle.

The controller reconciles drift continuously. Routine releases change Git and let the
controller converge. kubectl apply and helm upgrade are bootstrap and recovery tools,
not concurrent owners of the same application resources.

## Independently deployed MCP extensions

An extension packages its Kubernetes workload in its own Helm chart. The installation
adds a child application for that chart and registers the server, routes, capabilities,
and policy in the gateway control plane. This separates scheduling and rollout while
preserving one MCP authority and one authorization boundary.

An extension application normally selects two sources:

- the immutable OCI chart version;
- the enterprise Git repository containing values and the shared image lock.

Custom enterprise MCP servers follow the same pattern. They do not need Veoveo's build
system when their image and chart are already published. Their gateway entry still uses
the canonical typed control-plane model, internal assertion trust, policy checks, audit
path, and URI identities.

## Direct Helm

Argo CD is not a runtime dependency of Veoveo. An enterprise with another release
controller can render or install the same packages directly:

~~~bash
helm upgrade --install veoveo   oci://registry.example.com/veoveo/charts/veoveo   --version "$CHART_VERSION"   --namespace veoveo   --create-namespace   --values values/veoveo.yaml   --values values/images.lock.yaml   --wait
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
