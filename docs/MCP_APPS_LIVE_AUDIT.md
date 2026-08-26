# MCP Apps Live Audit

> Status: non-normative investigation register. No application, deployment, policy, or
> data fix is authorized by this document.
>
> Baseline observed: 2026-08-26 against `https://veoveo.bioma.ai/console/` and the
> `k3d-veoveo-bioma` cluster.

This register records behavior observed in the deployed MCP Apps, their Console host,
and the supporting Kubernetes installation. It separates demonstrated behavior from
inference and keeps proposed improvements beside the evidence that motivated them.

## Scope And Method

The baseline covered all 16 views discovered from the authenticated Apps catalog. The
views belong to 15 hosted servers because Optimization contributes two views. The
catalog reported no discovery degradation.

The investigation used the authenticated Console, browser network and accessibility
state, negative HTTP authorization checks, live App resources, live App frame source,
Console Audit and Access, and read-only Kubernetes inspection. The deployed core Helm
release was revision 88 with the revision label `5918f0287b18`; the UAV release was
revision 73 with the revision label `6edf9d6b`.

Before browser automation, the headed browser reported a visible 1920x1048 window.
WebGPU exposed SwiftShader and was rejected as hardware evidence. WebGL2 remained
hardware-backed throughout the visual checks with this renderer:

```text
ANGLE (NVIDIA Corporation, NVIDIA GeForce RTX 4090/PCIe/SSE2, OpenGL 4.5.0)
```

The audit made no code, configuration, policy, or cluster changes. It exercised safe
reads, Charts validation, Datasheet preview and statistics, a Time resolution example,
and a Recording query. It attempted an instruction that requested status only, but the
UAV App did not emit the request. It did not seal a recording, create a share, generate
media, launch an optimization, start a stream, create a view, change map data, or issue
a simulator command.

## Severity

| Severity | Meaning |
|---|---|
| Critical | immediate confidentiality, integrity, safety, or total-service failure |
| High | a primary workflow is blocked, an operational dependency is failed, or a meaningful security boundary is missing |
| Medium | a workflow is unreliable, misleading, difficult to discover, or exposed more broadly than necessary |
| Low | hardening, accessibility, cleanup, or usability debt with a practical workaround |

## Findings Register

All findings were open at the end of the baseline. Suggested improvements describe a
future outcome. They are not fixes made during this investigation.

| ID | Severity | Area | Finding and evidence | Potential improvement |
|---|---|---|---|---|
| MCPAPP-001 | High | App host contract | The Console iframe uses the contract-required `sandbox="allow-scripts"`. Native form submission is therefore blocked. Charts **Render**, Timeseries **Re-run forecast**, and UAV **Send instruction** produced no `tools/call` or agent-message request. Charts changed to `valid` after its input debounce while keeping the old preview. Live frame source binds those controls through `submit` handlers. The Map frame defines the same pattern for `create-layer-form`, `add-feature-form`, `import-artifact-form`, `acquire-source-form`, `save-view-form`, and `raw-admin-form`; those six paths are inferred to be affected. | Keep the restrictive sandbox and make App controls invoke their bridge handlers without depending on native form submission. Add browser acceptance for every visible submit action under the exact production sandbox. |
| MCPAPP-002 | High | DuckDB Workbench | The direct-launch Query template sends `db_id`, while the live `query` schema requires `db`. Execute has the same mismatch. Query returned 502 in the Console, and the DuckDB server logged `missing field db`. Ingest and Export start with `{}` despite requiring four and three fields respectively. | Align the App with the live typed schema. Let the user select or create an owner-visible database, then populate a valid query example such as `SELECT 1 AS value`. |
| MCPAPP-003 | High | UAV agents | All four pilot agents continuously use expired gateway credentials. In a ten-minute sample, the two gateway replicas rejected 9,211 requests to `/mcp/agent` with `ExpiredSignature`. Each pilot retried about 3.84 times per second and logged `Auth required`. The aggregate rate was about 15.35 failed requests per second. | Restore renewable agent authorization and use bounded retry backoff. Expose credential expiry and agent connectivity in the Cameras App and Console health surfaces. |
| MCPAPP-004 | High | Time Timeline | The App shows `ready` while `clock_quality.synchronized` is false, stratum is 16 against a maximum of 4, source diversity is 0 against a minimum of 2, and the uncertainty is the maximum unsigned 64-bit value against a 100,000,000 ns policy limit. The critical condition is buried in raw JSON. | Render a prominent policy-failure state with the violated limits. Prevent dependent workflows from mistaking an MCP-ready process for an operationally trustworthy clock. |
| MCPAPP-005 | High | Timeseries Forecasts | Direct launch shows only `Waiting for forecast data…`. There is no series, dataset, model, or example input. The live source requires a prior `ui/notifications/tool-input` before rerun and otherwise intends to report that the forecast tool must be invoked first. The form-submit defect prevents even that explanation from appearing. | Give direct launch a bounded example series and a source selector, or do not advertise it as a direct-launch App. Link to the forecast tool and describe the required invocation context. |
| MCPAPP-006 | High | Cluster isolation | Only three NetworkPolicies exist in namespace `veoveo`, and they select UAV workloads. No NetworkPolicy selects the core gateway, Console, data stores, or hosted MCP servers. Under Kubernetes NetworkPolicy semantics, those unselected pods are not isolated from namespace or cluster peers. | Add explicit default-deny and narrowly admitted ingress and egress for the core installation. Verify enforcement with negative connectivity evidence. |
| MCPAPP-007 | Medium | Console transport | App navigation encountered an Apps catalog 502 that recovered on retry. The Console BFF logged a pair of `Transport closed` retries followed by `console apps listing failed`, and the browser recorded repeated `ERR_QUIC_PROTOCOL_ERROR` messages. The first DuckDB load also exceeded 20 seconds. The evidence does not prove that QUIC caused the BFF transport closure. | Surface retry state in the App shell, retain the last good catalog, and add a transport health signal that distinguishes edge, BFF, gateway, and upstream failure. |
| MCPAPP-008 | Medium | Direct-launch scaffolding | Twenty-four live action templates across nine views omit at least one required top-level schema field. Frames, Optimization, Reason, and most Time actions begin with `{}`. Users must reconstruct complex schemas outside the App before the first useful call. | Generate typed forms or valid editable JSON examples from the tool schema. Populate identifiers from visible resources and provide Reset to example. |
| MCPAPP-009 | Medium | Resource presentation | Media Studio writes 995 models and 362,225 bytes of model JSON directly into the page. Recording Explorer writes 113 catalog entries and 50,285 bytes as raw JSON. Neither provides search-first cards, a table, paging, or click-to-populate behavior. | Default to a bounded result set with search, filtering, paging, and selection. Keep raw JSON behind an explicit inspection view. |
| MCPAPP-010 | Medium | Console routing | Browser history can change the hash without changing Console state. A reproduced Back action changed the URL from Time to `#/apps/uav-sim/live.html` while the heading and iframe remained Time Timeline. The Console initializes route state once and uses `replaceState` for navigation. | Synchronize view state with hash and history events. Add Back, Forward, and manual-hash browser acceptance. |
| MCPAPP-011 | Medium | Health semantics | The global header showed `2/2 platform services healthy`, and Cluster showed `29/29` workloads ready while every pilot agent was failing authorization and several App actions were unusable. Deployment readiness alone does not represent MCP, agent, clock, or App workflow health. | Add domain health for gateway sessions, agent authentication, clock policy, App catalog reads, and a small safe App canary. Distinguish process readiness from operational readiness. |
| MCPAPP-012 | Medium | Destructive actions | Recording Explorer places **Seal recording** beside a read-only query as another raw JSON action. The App did not present risk, permanence, a review step, or visible confirmation before execution. The action was not run during this audit. | Classify mutations and destructive operations in the App contract. Require an explicit review summary and confirmation for irreversible actions. Keep administrative tools visually separate from exploration. |
| MCPAPP-013 | Medium | Resource subscriptions | Media, Reason, and Optimization logged `resource is not subscribable` when their Apps loaded. Several App frames request subscriptions for resources that the server exposes only as reads. Cloudflared also logged repeated cancellations as navigation tore down App event streams. | Declare and request subscriptions only for resources that support them. Treat normal stream teardown separately from transport errors in logs and metrics. |
| MCPAPP-014 | Medium | UAV network policy | The `uav-sim-mcp` policy restricts port 8802 to the gateway but admits TCP 8803 with no `from` selector. The stream still requires its application-level viewer authorization, but the pod network boundary accepts traffic from any source allowed to reach the namespace. | Restrict the stream rule to the ingress controller and any named internal consumers. Retain short-lived per-view authorization as the application boundary. |
| MCPAPP-015 | Medium | Time data fidelity | The Time resource sends `18446744073709551615` on the wire. The App parses it as a JavaScript Number and displays `18446744073709552000`, which is not a safe integer. Other 64-bit temporal values can suffer the same loss. | Preserve large integers as strings or typed bigint-safe values through the App bridge and renderer. Test exact round trips at the schema bounds. |
| MCPAPP-016 | Low | Kubernetes identity | The common ServiceAccount has token automount disabled, and only Console BFF receives an explicit projected token. This is good containment. The cluster-reader Role is nevertheless bound to the same identity named by nearly every core pod. | Give Console BFF a dedicated ServiceAccount for cluster inventory. Leave hosted servers on identities without Kubernetes API roles. |
| MCPAPP-017 | Low | Recording token hygiene | `rerun.redap_token` remained in origin localStorage after leaving Recordings. The observed token was expired, recording-scoped, and host-bound, so it no longer granted access. It was still JavaScript-readable and persisted across later App views. | Remove delegated viewer tokens on expiry and when playback closes. Show the delegated session lifetime in diagnostics without exposing the token. |
| MCPAPP-018 | Low | Accessibility | View Preview exposes Altitude and Azimuth spinbuttons with accessibility minima and maxima both reported as zero. The Distance spinbutton reports `invalid=true` for the valid displayed value 650. The Audit search textbox has no accessible name. | Give every numeric control exact accessible bounds and associate the Audit search field with a visible label. Include iframe content in accessibility acceptance. |
| MCPAPP-019 | Low | Navigation density | Every visited App server remains expanded in the primary navigation. The resulting list adds 31 server and view rows to every Console page. Generic titles such as Workbench and Workspace rely entirely on their group heading for identity. | Default groups closed outside Apps, preserve only deliberate expansion, and include the server in ambiguous accessible names. Add search or recent Apps when the catalog grows. |

## App-By-App Record

| App | Demonstrated behavior | Gaps and improvement opportunities |
|---|---|---|
| Artifact / Library | Loaded the authorized index and reported an empty artifact collection. | Explain how artifacts arrive, link to the platform Artifacts view, and select an artifact into metadata, release, and sharing actions. The current blank identifiers offer no first success path. |
| Charts / Composer | Loaded a valid inline region/revenue dataset and automatically rendered a Vega-Lite chart. Backend, chart type, dimensions, theme, and field controls were clear. | Manual Render is blocked by MCPAPP-001. Keep the strong default, then prove rerender, compile, and output copy with changed input. Link `author_flint_chart` for a guided prompt path. |
| Datasheet / Workbench | Supplied a small city/value CSV by default. Preview returned three typed rows, and Column stats returned useful statistics. | This was the clearest direct-launch example. An artifact picker and links to `datasheet-profile-dataset` and `datasheet-report-review` would extend the successful first run. |
| DuckDB / Workbench | Listed the authorized database resource, which was empty. | Query and Execute use the wrong field name. There is no guided database creation or ingest path. The direct-launch sample cannot succeed. |
| Frames / Workspace | Listed two UAV frame worlds. | Convert, Create, Publish, and Batch all start with `{}`. Offer a world selector, example points, a visual frame tree, and links to the three Frames prompts. |
| Map / Workspace | Loaded a purpose-built map UI and correctly reported hardware NVIDIA WebGL2. Two visible layers and one saved view were available. | Both layers showed zero visible records and the canvas was blank. Seed a useful governed example or explain the empty dataset. Six mutation forms are inferred to be blocked by MCPAPP-001. |
| Media / Studio | Loaded the provider model catalog. | The 995-model raw dump dominates the page. Search, model selection, schema inspection, generation templates, and the four Media prompts should form one guided path. |
| Optimization / Models | Displayed solver profiles and existing problem and solution resources. | Convex and MILP inputs start empty. Provide small valid models, policy presets, and a direct link to `formulate_mathematical_model`. |
| Optimization / Routes | Displayed the NVIDIA RTX 4090 capability and cuOpt 26.08 backend. | Route and scenario inputs start empty. Add a compact vehicle-and-stop example, policy presets, Map travel-model selection, and links to the routing prompts. |
| Reason / Analyses | Loaded Analyses, Pipelines, and Models resources. The analyses collection was empty. | Analyze recording starts with `{}`. Select from governed recordings, pipelines, and models, offer example questions, and link the two Reason prompts. |
| Recording / Explorer | Loaded 113 authorized recordings. A read-only query with a selected recording ID completed and returned an empty row set. | Present the catalog as a searchable table. Let selection populate the query, discover timelines and entity paths, explain empty results, and guard Seal recording as described in MCPAPP-012. |
| Stream / Live Monitor | Presented a clear default pipeline, counters, session state, and Start live session. | A session was not started because it creates GPU work. Add a small description of expected input and output plus links to the two Stream prompts. |
| Time / Timeline | Loaded current time and successfully resolved a manually authored RFC 3339 example. | Five actions use raw JSON, four without required fields. Highlight the failed clock policy, preserve 64-bit values, provide examples, and link the three Time prompts. |
| Timeseries / Forecasts | Initialized the view. | Direct launch never acquires forecast input, and Re-run is also blocked by the form sandbox. Provide example data or make the invocation dependency explicit. |
| UAV Sim / Live Cameras | Displayed five healthy live views with current city imagery. The server correctly identified NVIDIA NVENC, and the browser correctly labeled software H.264 decode. | Send instruction is blocked by the form sandbox. All target agents are also disconnected by expired credentials. Show per-agent connectivity before accepting an instruction. |
| View / Preview | Supplied a Google Photorealistic layer, Statue of Liberty camera defaults, three modes, and three useful presets. | No persistent view or capture was created during the audit. Correct the numeric accessibility state and explain which actions persist a composition or artifact. |

## Default Data, Templates, Prompts, And Resources

Twelve app-capable servers advertise prompts, but their Apps do not link those prompts
into the task flow. All 15 advertise resource templates except Charts. A user can see
the App and the MCP server separately, but the direct-launch experience rarely connects
the two.

The next design pass should apply these checks to every direct-launch view:

- The first screen states what the App can accomplish and what authority it will use.
- A safe example or governed selectable resource can produce a useful result within one
  minute.
- Required schema fields are present in the default editor, with valid example values
  where an identifier is not installation-specific.
- Resource selection populates downstream actions instead of asking the user to copy
  opaque identifiers.
- Relevant MCP prompts and resource templates have visible links with short explanations.
- Empty states explain how data arrives and link to the Console surface that owns it.
- Large collections default to search, filters, tables, or cards rather than raw JSON.
- Read-only, mutating, administrative, and irreversible actions are visually distinct.
- A failed prerequisite, such as no database, no prior forecast input, or a disconnected
  agent, disables the action with an explanation.
- Each action reports request, progress, success, error, and retry state in the App.

The Datasheet default is the best baseline pattern. Charts also has a strong initial
sample, and View has useful location presets. Stream provides a clear empty state and a
safe default pipeline. Those patterns can guide the generic resource-and-action Apps
without making each one bespoke.

## Access And Security Verification

The tested browser boundary failed closed in the cases exercised:

| Check | Result |
|---|---|
| Unauthenticated `/console/api/apps` | 401 |
| Unauthenticated App frame | 401 |
| Unauthenticated App tool POST | 401 |
| Authenticated mutation without CSRF | 403 |
| App URI whose owner does not match the declared server | 400 |
| Unknown tool through an otherwise valid App | 404 |
| Undeclared foreign resource read | 403 |
| Unknown App frame resource | 404 |

The frame response used `default-src 'none'`, denied objects and base URLs, limited
images and media to data or blob sources, and allowed no general network destination.
The iframe used `sandbox="allow-scripts"` and `referrerpolicy="no-referrer"`. The parent
did not expose a gateway bearer to the frame. Authenticated App calls carried the
HttpOnly session path and CSRF token through the host bridge.

Cluster containers ran as non-root with read-only root filesystems, privilege escalation
disabled, and all Linux capabilities dropped. Images were digest-pinned. GPU workloads
requested NVIDIA GPU resources, and the visual checks used the RTX 4090 rather than a
software renderer. These passes do not remove MCPAPP-006, MCPAPP-014, or MCPAPP-016.

Console Audit found the exercised `charts:validate_chart` call with the authenticated
actor, `Succeeded` outcome, `tools_call` action, gateway resource, and trace identifier.
Console Access showed owner membership and the active policy revision, but it did not
show the active OAuth profile, granted scopes, or the rule that made each App action
visible. Showing that explanation would improve security transparency, especially for
administrative actions.

## Cluster And Operational Notes

All current Deployments and StatefulSets were available at the end of inspection. The
Console inventory showed 29 of 29 workloads ready, 29 of 29 current pods ready, and 20
total restarts. Kubernetes showed 35 pod objects because six completed rollout or
installation pods remained in the listing.

The restart evidence was not treated as an active outage by itself:

- Optimization restarted twice while its main container waited for the cuOpt executor
  socket.
- `uav-sim-mcp` restarted five times during an earlier SurrealDB DNS failure.
- Two pilot pods restarted six times after losing scheduler leases during an earlier
  overlap.

These components were running when inspected. Startup dependencies and lease turnover
would still benefit from explicit readiness gates and clearer Console history. The
ongoing expired-token storm in MCPAPP-003 is an active failure and is not explained by
those earlier restarts.

## Retest Checklist

Future entries should append a date and retain the original evidence. Close a finding
only after its live behavior is reproduced successfully.

- Exercise every visible App action through the production `allow-scripts` sandbox.
- Run DuckDB Query and Execute from a fresh identity with no existing database.
- Send a harmless status instruction to each pilot and verify one audited delivery per
  request with no expired-token retry storm.
- Confirm the Time App changes from warning to trusted only when its clock policy passes.
- Launch Timeseries directly from the catalog and produce a forecast without external
  schema reconstruction.
- Run unauthorized, CSRF, owner-mismatch, cross-resource, and unknown-tool negatives.
- Prove default-deny pod connectivity and the narrow UAV stream ingress path.
- Use Back, Forward, and a pasted App hash without reloading the document.
- Verify exact 64-bit values through the frame and accessibility bounds through the
  browser tree.
- Confirm large catalogs remain usable at the current 995-model and 113-recording scale.
