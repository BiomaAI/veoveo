# Repository Tooling Design

Repository tooling composes owner declarations and native commands through `cargo xtask`.

Image planning captures the Computer provider manifest SHA-256 from the selected source
checkout when the dependency graph includes `computer-provider`. The optional provider
input records that digest separately from Cargo compiler families. Bake receives the
selected context and digest together; resolved context or argument changes reject before
execution. The provider source stage checks the packaged manifest against that argument
before compilation, and the runtime OCI label consumes the same argument.

## Standards And Protocols

| Format or interface | Supported profile |
| --- | --- |
| Cargo metadata and JSON compiler messages | Package IDs, native target kind/name, selected features, executable and linked-library paths from the current compiler invocation |
| `veoveo.ai/smoke-scenarios/v1` | Closed version tag, camelCase controlled fields, checked owner descriptors under tracked or nonignored `smoke/scenarios.json` paths |
| `veoveo.ai/smoke-artifacts/v1` | Closed version tag, camelCase controlled fields, source identity and immutable executable/library SHA-256 receipts |
| `veoveo.ai/framework-outcome/v1` | Exactly one passed libtest, pytest or Node framework case, with zero failed or skipped cases |
| Linux process groups and signals | PID/start-time ownership, one execution deadline and one cleanup grace |

## Smoke Composition

The dispatcher discovers typed descriptors and verifies their owner, package, target, feature prerequisites and source containment against Cargo metadata. New server onboarding adds an owning descriptor and target without editing a shared scenario enumeration. Installed registry fixtures compile in Gateway or Bioma composition; domain-neutral shared support contains process and maintained-client mechanics.

Descriptor inventory, locked Cargo metadata, effective-feature resolution, installations, framework listing and owner execution consume one deadline. Inventory and declaration help have a 30-second limit and execute no owner. Selecting a scenario applies its deadline to the original start time.

Cargo resolves every selected root and prerequisite before building or installing dependencies. The dispatcher compares the complete normal/build closure and the normal/test-dev projection for individual roots and candidate groups. Compatible roots share a Cargo invocation only when both projections equal the union of the individually admitted sets. Conflicting declarations reject; incompatible roots build in separate groups. Native builds preserve Cargo's default product layout and the configured external target root. Graph queries select `host-tuple` to distinguish native host dependency features from target features. This execution profile requires Cargo's implicit host target. `CARGO_BUILD_TARGET`, an effective `build.target` setting, or configuration includes reject before native preparation; explicit target profiles require separate declaration and qualification.

Compiler receipts must equal both admitted feature projections. Build-script and procedural-macro target kinds identify host work. Materially identical native sets share a context; unresolved differing sets reject instead of claiming a distinction the compiler messages do not establish. Compiler messages identify hashed test executables and linked-library locations. The same admitted runtime libraries configure exact listing and execution, and replacement checks run again before dispatch.

Python owners declare a package identity, locked uv environment, Hatch module roots and selected locked extras or groups. Node owners declare matching package and lock-root identities and an owned source file. A checked executionOwner declaration can delegate a composition's harness to another component with a stated reason. Source and manifest containment still apply. Declarations convey prerequisites, not effect authorization.

Exact framework selections accept no additional framework arguments. Libtest output must identify the requested case as the sole executed passing case, as well as report one pass and zero failed or ignored cases. Filtered cases represent the unselected tests. Pytest and Node use their maintained framework adapters.

Cancellation reaches the owning process group and registered nested local groups. Cleanup receives one grace interval; repeated signals cannot extend it. A failed or uncertain outcome preserves private diagnostics and leaves remote reconciliation to the owner. The dispatcher does not infer remote cleanup from local process death.

Generic conformance and certify retain their protocol commands and bearer input. Gateway composition owns the complete schema export used by offline bundles; generic conformance owns its certification profile/report pair. Owner support utilities reuse the existing internal token issuer.


## Helm Release Evidence

`veoveo.ai/helm-chart-release-evidence/v1` records the selected chart archives and
optional OCI publications. Its controlled fields are camelCase. Archive digests use
the deployment-owned `ArtifactDigest` profile with the emitted `sha256:` prefix;
file production, immutable copies and evidence decoding share that admission. Local evidence
reads admit one closed current document within 64 KiB and require the same release,
source revision, Helm version, chart selection, archive filenames and digests
before adding OCI coordinates. An existing publication cannot be replaced by
enrichment. Before invoking Helm, publication admits the complete retained evidence,
selected archive bytes and chart/release OCI coordinates through the deployment
Artifact coordinate type and URL component checks. Recorded publications refuse a
repeat push. The final digest enrichment is written after the registry response.
Chart names and Helm archive/OCI spellings keep their native profiles;
this wrapper does not establish that a chart was installed.
