# Installation-neutral qualification follow-up

Bioma is one Veoveo installation reference. This register tracks test and guide paths
that currently assume its identity. It is a follow-up after the active component
upgrade and Redap qualification goal; it does not change that goal's acceptance target.

| Location | Current installation assumption | Follow-up |
|---|---|---|
| `testing/smoke/src/bin/smoke/scenarios/recording_catalog_sdk.rs` | Rejects Kubernetes contexts other than `k3d-veoveo-bioma` and maps `veoveo.bioma.ai` into the native SDK container. | Take the context, public hostname, local ingress port and service identity from an explicit installation profile. Preserve Rerun's token host check. |
| `testing/smoke/src/bin/smoke.rs` | Several installed scenarios default to the Bioma context and hostname. | Put installation-specific defaults in `examples/bioma` or a selected profile; keep the generic smoke commands parameterized. |
| `testing/deployment-smoke/src/helm_config.rs` | Bioma fixture assertions and general chart assertions share one module. | Keep Bioma assertions as reference-installation fixtures and make the generic chart contract independently runnable. |
| `docs/RERUN_RECORDINGS.md` | The command example and local hostname mapping name Bioma. | Separate a generic service-token procedure from a clearly labeled Bioma example. |

The follow-up passes when an independent installation can run the catalog SDK smoke
with its own hostname and service account without editing Veoveo source, while Bioma
continues to run from its reference profile.
