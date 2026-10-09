# OpenShell Release Profile

## Standards And Protocols

This package consumes NVIDIA OpenShell **0.1.2** protobuf/gRPC and its released
Linux/amd64 executables. The release commit is
`6648bd0c290efbc41ba131ee9831ee45cd431f94`, with source tree
`26f142d5c3be825e72f4d1a705ecf6aa5d4d6b10`.
[`manifest.json`](manifest.json) declares the closed
`veoveo.ai/openshell-release-profile/v1` installation packaging profile.
The [protocol provenance](../protocol/provenance.json) records the vendored public
protocol inputs separately.

## Official Artifacts

The Docker package downloads four official
[release assets](https://github.com/NVIDIA/OpenShell/releases/tag/v0.1.2), verifies
each archive and executable SHA-256, and copies their unchanged bytes into the
pinned official gateway image. CLI and sandbox use the published musl executables;
gateway and supervisor use GNU executables. Packaging runs no provider compiler.
The gateway includes the Docker driver in process. It starts no separate driver.

The manifest pins the official gateway and supervisor image index, amd64 manifest
and config digests. Installation mirrors preserve the selected amd64 manifest and
config bytes. Host admission checks the supervisor's manifest digest and local
config identity without requiring private labels on the upstream image.
The official supervisor image contains `/openshell-supervisor`; the gateway Host
supplies the separately released sandbox executable for workload injection.

## Authentication And Configuration

The installation supplies an HTTPS OIDC issuer, an exact audience and a dedicated
private worker OAuth client with the declared OpenShell user/admin roles. The Host
keeps stock RBAC enabled, disables certificate-to-user promotion and refuses
anonymous user access. Supervisors retain the complete TLS bundle and mandatory
gateway-issued Sandbox JWT. Worker credentials never enter guest configuration.
The stock shared client certificate provides transport authentication; its issuer
or subject does not establish provider user authority.

Veoveo's runtime owns bearer acquisition, refresh and RPC metadata. The Computers
service continues to authorize each domain operation. Configuration and artifact
admission establish prerequisites; they do not qualify provider authentication,
containment, retained lifecycle or installed rollout.

## Lifecycle And Guest Policy

Stock gateway restart replaces the supervisor and revives the guest. Terminal
sessions reset while retained-volume file contents persist. Veoveo preserves
uncertain operations until the provider supplies correlated settlement.
Owned immutable guest images exclude supplementary image-account groups. The
private Docker daemon sets its local log driver to three files of 10 MiB each.
Retained volume initialization uses stock Docker copy-on-mount behavior or an
allocator-owned step outside the provider.

Qualification requires positive worker and Sandbox authentication, negative
certificate-only/anonymous/wrong-issuer/wrong-audience/expired-token controls,
retained replacement and the selected guest identity and PTY behavior. A package
build or compiler check does not authorize activation. GPU workloads require their
separate hardware qualification.

## Upstream Reports

The local [PTY ownership draft](upstream-issues/pty-ownership.md) and
[Landlock device-path draft](upstream-issues/landlock-device-path.md) describe
qualification cases for the published release. Both require reproduction against
the official executable. They have not been submitted and authorize no provider
changes.
