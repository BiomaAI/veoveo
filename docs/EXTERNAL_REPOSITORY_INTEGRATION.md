# External Repository Integration Runbook

This runbook is the supported procedure for a coding agent integrating an independently
owned repository with a Veoveo installation. The agent coordinates existing release
artifacts and standard build or GitOps tools. Veoveo does not require a deployment
coordinator, a shared source checkout, or a prescribed external build system.

## Standards And Protocols

| Standard or protocol | Boundary use |
|---|---|
| OCI Distribution Specification | private image, Helm chart, conformance, SBOM, and provenance artifacts addressed by SHA-256 digest |
| Helm and Kubernetes | extension packaging and installation rendering |
| Git | reviewed installation desired state and rollback identity |
| JSON Schema 2020-12 | compatibility, extension-release, gateway fragment, binding, and controlled configuration validation |
| Model Context Protocol | hosted-server behavior selected by the Veoveo compatibility manifest |
| `veoveo.io/compatibility-manifest/v1` | exact Veoveo contracts and distributable integration artifacts |
| `veoveo.io/extension-release/v1` | independently published extension release |
| `veoveo.io/gateway-server-fragment/v1` | extension-owned server contribution |
| `veoveo.io/gateway-binding/v1` | installation-owned exposure and authorization |
| SHA-256 | immutable file and OCI manifest identity |

## Agent Boundary

The extension repository owns source compilation, tests, its container image, its
application chart, domain smoke scenarios, a gateway fragment, and an extension-release
manifest. The installation repository owns the selected releases, private coordinates,
bindings, policy, Secrets, platform components, image digests, and GitOps objects.

The agent may use Cargo, npm, uv, Make, Bake, or repository-local compiled tooling when
that source already uses it. The agent must not copy Veoveo's `xtask`, join the Veoveo
workspace, edit the Veoveo chart, or make the extension responsible for a complete
gateway control plane.

Credentials remain in the installation's credential helpers, environment, or Secret
manager. They do not enter a manifest, command transcript, generated file, or agent
message.

## Required Inputs

Before changing an extension, obtain these installation-owned values:

| Input | Requirement |
|---|---|
| Compatibility manifest | immutable coordinate plus expected SHA-256 digest |
| Schema bundle | digest selected by that manifest |
| Python package source | authenticated private index when the Python SDK is selected |
| OCI registry prefix | authenticated private image and chart destination |
| Installation origin | canonical private or public user-facing URL |
| Extension identity | stable DNS-style identifier and server slug |
| Gateway base and binding owner | installation repository paths and reviewing authority |
| Platform selection | enabled Veoveo components, MCP servers, and artifact audiences |

Stop when the compatibility artifact cannot be resolved by digest, its hash differs, or
the requested SDK runtime falls outside the manifest's supported range.

## Extension Repository Procedure

### 1. Pin the compatibility surface

Copy the resolved compatibility manifest and its expected digest into the extension's
release input directory. Validate it against the bundled schema. Record the
compatibility release identifier in the extension-release manifest.

For Python, configure the authenticated index outside source control and retain the
exact `veoveo-mcp` version selected by the manifest. Commit the repository-native lock.
The canonical template uses:

```sh
uv lock
uv sync --locked --all-extras
uv run pytest
```

Another language keeps its native lock and tests. A Rust repository is not required to
consume a Veoveo Rust crate unless a compatibility release explicitly publishes one.

### 2. Implement and test the hosted server

Keep the domain protocol and tests beside the server. The implementation must satisfy
the hosted MCP contract revision in the compatibility manifest and publish its required
well-known documentation and contract resources.

Run unit, integration, schema, and policy tests using the repository's ordinary
commands. Domain smoke scenarios remain local to the extension and must not import
Veoveo server crates or examples.

### 3. Publish an immutable image

Build with the repository's native image graph. Pass private package-source coordinates
through BuildKit secrets or equivalent credential-safe inputs. Push the image to the
installation-selected registry, attach the repository's SBOM and provenance, and record
the returned OCI manifest digest.

The extension-release manifest uses the digest-addressed image coordinate. A mutable
tag, including `latest`, is not release evidence.

### 4. Certify the running server

Start the digest-addressed image in a representative environment. Adapt
`mcp/conformance/profiles/hosted-server.example.json` to the extension's exact slug,
owned schemes, endpoint, and advertised surfaces.

Run the native `certify` artifact or the digest-addressed conformance OCI image selected
by the compatibility manifest:

```sh
veoveo-mcp-certify \
  --profile conformance/hosted-server.json \
  --report release/conformance-result.json
```

Provide the gateway internal bearer only through `MCP_BEARER_TOKEN`. Hosted
certification always uses it for both MCP and the same-origin administrative docs
projection; the credential never enters the profile or report. Store the report as an
immutable release artifact. Any failed check blocks the release.

### 5. Package the application chart

Use the exact private `veoveo-extension` library version selected by the compatibility
manifest. Commit `Chart.lock`, then run the chart's ordinary Helm gates:

```sh
helm dependency build deploy/helm
helm lint deploy/helm --values deploy/helm/values.test.yaml
helm template extension deploy/helm \
  --values deploy/helm/values.test.yaml \
  --set veoveo.production=true \
  --set-string veoveo.imageDigest="$IMAGE_DIGEST" \
  > release/rendered.yaml
```

The production render must use immutable image digests and restricted security
contexts. A GPU workload must request its required NVIDIA resource and fail closed when
the runtime is unavailable.

Package and push the chart through the authenticated Helm OCI client. Record the chart
manifest digest returned by the registry.

### 6. Publish the extension contribution

Validate the extension-owned gateway fragment against the schema from the compatibility
bundle. The fragment declares capabilities and platform requirements. It does not
contain tenants, installation policy, credentials, or authorization grants.

Create `veoveo.io/extension-release/v1` from the exact source revision and immutable
artifact identities. The anonymous shape is
`extensions/examples/anonymous.extension-release.json`. Validate both the generated
JSON Schema and the typed cross-field rules before publication.

The completed extension release contains:

- the digest-addressed application image;
- the OCI chart coordinate and digest;
- the gateway fragment coordinate and digest;
- one passing conformance result;
- the selected Veoveo compatibility release;
- an optional canonical simulation-base requirement.

## Installation Repository Procedure

### 1. Select the release

Resolve the extension-release manifest and verify its digest. Check that its required
compatibility release matches the selected Veoveo release. Copy no source tree into the
installation repository.

Add or update the extension's GitOps Application, values, chart version, and image
digest. Keep the platform chart and extension chart as separate releases.

### 2. Author installation policy

Create an installation-owned gateway binding for the fragment's server. Select exposed
tools and resources, required scopes, tenants, artifact audiences, and data-label
policy. The binding may reduce the extension contribution. It cannot add a capability
the fragment did not declare.

Compose the base, fragment, and binding with the native or digest-addressed
`gateway-compose` artifact:

```sh
gateway-compose \
  --base gateway/base.json \
  --fragment gateway/fragments/extension.json \
  --binding gateway/bindings/extension.json \
  --output gateway/control-plane.json \
  --requirements gateway/requirements.json \
  --provenance gateway/provenance.json
```

Commit all three outputs. Composition must reject route, mount, MCP path, URI-scheme,
resource-ownership, or policy collisions.

### 3. Satisfy platform requirements

Read the generated requirements before rendering Helm:

| Declared capability | Required platform selection |
|---|---|
| `artifact` | Artifact MCP and artifact service |
| `frames` | Frames MCP |
| `map` | Map MCP |
| `media` | Media MCP |
| `recording` | Recording MCP and Recording Hub |
| `rrd` | Recording MCP, Recording Hub, and producer-side recording forwarder |

The selected artifact audience set must include every audience in the requirements
document. Frames, Map, Media, and Recording remain separate platform services. They are
not satisfied by compiling their libraries into an extension or simulation image.

### 4. Render and commit desired state

Run `helm dependency build`, `helm lint`, and `helm template` for both releases using
the installation values. Inspect every rendered container and init-container image.
Production images use `repository@sha256:digest`.

Commit the digest-pinned values, gateway outputs, provenance, and GitOps objects in one
reviewed installation change. The existing reconciliation controller applies that
commit. Direct Helm remains valid when the installation does not use GitOps.

### 5. Verify the installed release

Require successful platform and extension rollout, gateway discovery, OAuth metadata,
MCP capability discovery, and extension-owned domain smoke scenarios. A required GPU
workflow must prove hardware-backed execution.

Upgrade changes the extension release and digest pins without rebuilding the platform.
Removal deletes the extension release and binding, recomposes the gateway, and leaves
the platform release intact.

## Copyable Agent Instructions

An external repository may place the following concise policy in its own `AGENTS.md`:

```text
VeoVeo integration is artifact-based. Read the pinned compatibility manifest before
changing the server or release inputs. Use this repository's native build, test, smoke,
and image commands. Do not join the VeoVeo workspace, run veoveo-xtask, or edit VeoVeo
source.

Pin SDKs, schemas, images, charts, conformance tools, and gateway tools to immutable
versions or SHA-256 digests selected by the compatibility manifest. Keep credentials
outside source control and command output.

The extension owns its image, chart, gateway fragment, conformance result, and
extension-release manifest. The installation owns bindings, authorization, Secrets,
artifact coordinates, platform selection, and GitOps desired state. Fail the release
when schema validation, conformance, production Helm rendering, digest verification, or
required hardware acceptance fails.
```

## When More Tooling Is Justified

This runbook is the coordination layer. Do not add a Veoveo deployment wrapper merely
to sequence these commands. A new shared tool is justified only after repeated
installations demonstrate the same logic cannot be represented by existing schemas,
typed validators, Helm, OCI clients, and GitOps review.
