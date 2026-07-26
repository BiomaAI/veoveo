# Gateway Composer Design

## Standards And Protocols

| Standard or protocol | Supported profile |
|---|---|
| `veoveo.io/gateway-server-fragment/v1` | extension-owned hosted-server capabilities and platform requirements |
| `veoveo.io/gateway-binding/v1` | installation-owned profile exposure, authorization, artifact audiences, and recording producers |
| `veoveo.io/gateway-composition-provenance/v1` | exact input hashes, output hash, requirements, and contributed objects |
| JSON Schema 2020-12 | generated controlled document schemas |
| SHA-256 | exact input and output byte identity |
| Model Context Protocol | final control plane follows `mcp/contract/DESIGN.md` |

## Responsibility

The composer joins extension-owned capabilities to installation-owned authority without
contacting Git, a registry, Kubernetes, Helm, or a running gateway. It accepts one
complete base control plane and matched fragment/binding pairs. The final ordinary
`GatewayControlPlane` passes the canonical validator before any output is written.

The binary also emits aggregate platform capabilities and artifact audiences for the
deployment resolver. Provenance contains only stable identities, schema versions,
object summaries, and content hashes. File paths, credentials, and Secret values are
not copied into it.

## Determinism

Inputs are matched by server slug and sorted before composition. Added servers, profile
exposures, policy rules, recording producers, provenance inputs, and contribution
summaries use stable ordering. The same input bytes produce byte-identical control
plane, requirements, and provenance files.

The extension cannot expose itself or grant policy. A selected fragment without one
installation binding fails. An unmatched binding fails. Required artifact audiences
and recording producer declarations fail until the binding admits them.
