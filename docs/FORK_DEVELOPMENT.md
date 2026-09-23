# Fork Development

Developers customize Veoveo in a fork of this repository. The fork contains the
implementation, tests and build graph. Installation configuration may live in a
separate Git repository; Bioma provides the maintained reference configuration.

## Standards And Protocols

| Standard or protocol | Supported use |
|---|---|
| Git | reviewed upstream merges, immutable build revisions and retained component snapshots |
| MCP | hosted-server profile in `mcp/contract/DESIGN.md`, including Tasks and subscriptions |
| MCP Apps | domain UI discovery and interaction through the existing gateway and clients |
| OCI Distribution | digest-pinned images and application charts, with build provenance |
| Helm and Kubernetes | application releases with explicit ownership, security and GPU requirements |
| `veoveo.io/deployment/v8` and `veoveo.io/deployment-lock/v8` | local source publication, typed component selection and immutable artifact reuse |
| SurrealDB 3.2.4 | separate checksummed upstream and downstream migration histories |

## Code Placement

Use `docs/CODEMAP.md` to find the component that owns a change. A new hosted Rust
server belongs under `servers/` and joins the Cargo workspace. Python servers use
`templates/python-mcp` and import `sdk/python` from the same checkout. Domain Apps
live beside their server. Changes to the Console, Workspace, gateway or agent runtime
use those components' existing modules.

A directory does not require its own service. Choose a separate process or image
when privilege, failure isolation, scaling or release cadence warrants one. Shared
Helm helpers live in `deploy/helm/common` and are bundled into application charts
through local file dependencies. They have no separate distribution promise.

## Upstream Merges

Keep `origin` pointed at the fork and add the upstream repository as `upstream`.
Develop focused changes on topic branches and preserve the history of shared branches.
Review upstream changes on a branch before integrating them:

```sh
git fetch upstream
git switch -c integrate-upstream
git merge --no-ff upstream/main
```

Resolve conflicts in code, schema and deployment configuration together. Review the
upstream migration sequence before running it against downstream data. A clean Git
merge does not establish that two schema changes compose safely.

Run the owning component checks and record them with `cargo xtask test-report run`.
Review `cargo xtask test-report show`, including each check's input scope. Changes to
shared contracts broaden the checks and images affected. Commit the report and its
new receipt files with build-input changes. Integrate the reviewed branch using the
fork's normal pull-request process.

`deploy/contract/tests/fork_installation.rs` exercises a downstream workload followed
by an upstream merge. It verifies that the workload survives and that the installation
can retain an earlier platform image revision while updating the workload release.

## Python Development

Copy `templates/python-mcp` into `servers/<domain>-mcp`, rename its domain package and
update its local `veoveo-mcp` path. Keep the committed third-party lockfile current
with the selected SDK. Run native commands from the project directory:

```sh
uv sync --locked --all-extras
uv run --locked --all-extras pytest -q
```

The Docker build context is the repository root. Copy the SDK and server into that
context and install from the committed lock with `--no-editable`. The template's
Dockerfile and root Bake target demonstrate this path. Installation package mirrors
may replace public dependency sources through normal uv configuration; Veoveo's own
SDK does not require a separate package index or compatibility publication.

## Registration And Authority

Add server definitions and Apps to the complete typed gateway control plane. The
installation selects endpoints, exposure and policies. A source change grants no
runtime privileges: internal assertion verification, tool grants, task ownership,
artifact audiences and audit rules apply to fork code as they do to upstream code.

Remote MCP servers and protocol bridges use their existing registration and
authentication contracts. MCP Apps and Tasks work the same in fork code as in
upstream code.

## Build And Deployment

Use the root Bake graph and the existing `cargo xtask image` and `release` commands.
Build affected targets and keep unchanged image digests. Each source entry in a
deployment profile owns a set of components. To publish only some components, check
out the revisions they need; publication reads only the local checkouts you select.

Publish application charts with their bundled library dependency. Pin images and
charts in installation values, render the resulting resources, and commit desired
state for the installation's reconciliation controller. Component-scoped updates keep
unrequested releases and their image provenance intact. See
[`IMAGE_BUILDS.md`](IMAGE_BUILDS.md) and
[`ENTERPRISE_DEPLOYMENT.md`](ENTERPRISE_DEPLOYMENT.md).

## Managed Kernel Upgrades

Changing a managed-kernel image changes its approved runtime-template revision.
Coordinate that change as a drained upgrade: the manager retires workloads whose
previous template approval no longer matches the installation. Helm readiness alone
does not complete the agent upgrade.

After the installation exposes the new template, save and publish the affected
agent definition with its current `templateRevision`. Preserve its instructions,
parameters, tools and audience. Apply the published revision to each existing
instance with the lifecycle API's `revision` change and its expected generation.
The manager accepts the new template only if its image is the only change; every
other authority, configuration, and storage field must match the previous template. Wait for each
instance's active revision and generation to match the request and its phase to
become `ready`.

The controller replaces Kubernetes Deployments during this transition. Instance IDs,
OAuth identities, signing Secrets and memory claims stay attached to the existing
instances. Creating replacement instances or deleting retained memory is unnecessary.

## Downstream Data Migrations

Append fork SQL and entries to `platform/store/downstream/catalog.rs`. That catalog
starts empty upstream and numbers migrations from zero independently of upstream.
Each entry declares the upstream version it needs. The normal store runner validates
both histories, applies pending upstream steps, then applies pending fork steps.
A deployed entry cannot be edited, removed or renumbered. Correct it with a new step.

The [migration runner design](../platform/store/src/migrations/DESIGN.md) specifies
transaction rollback, upgrade ordering and recovery. A fork must test retained data
against each upstream merge. When a change needs preparatory data conversion before
an upstream migration, perform that conversion in a coordinated maintenance step.
Drain the previous migration owner before upgrading its catalog.

## Rollback

Before promoting a release, keep the previous deployment's image and chart pins and
its installation commit. Rolling back to them restores the deployment, but not the
database: an image rollback does not reverse a migration. Test and plan recovery for
each database change separately.
