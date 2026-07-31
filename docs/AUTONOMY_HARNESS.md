# Autonomy Harness

An Autonomy Harness is the enforced operational boundary that allows agents to wake,
reason, and act without synchronous operator supervision. “Left loose” describes an
agent's freedom to schedule work inside that boundary. It never means unrestricted
credentials, network access, compute, data access, or authority.

The capability architecture appears in the
[browser edition](autonomy-harness.html) and
[print edition](autonomy-harness-print.html). This document defines the shared
responsibility and operational proof that keep continuously autonomous agents inside
their installation-owned containment boundary.

## Standards And Protocols

| Standard or protocol | Harness boundary |
|---|---|
| [NIST AI RMF 1.0](https://doi.org/10.6028/NIST.AI.100-1) and [NIST AI 600-1](https://nvlpubs.nist.gov/nistpubs/ai/NIST.AI.600-1.pdf) | Risk ownership, testing, evaluation, validation, verification, incident learning, and lifecycle reassessment. This document uses the framework as a risk-management lens and does not claim NIST certification. |
| [Kubernetes Pod Security Standards](https://kubernetes.io/docs/concepts/security/pod-security-standards/) and [NetworkPolicy](https://kubernetes.io/docs/concepts/services-networking/network-policies/) | Workload hardening and network isolation on the installation-pinned Kubernetes release. The agent runtime must satisfy the Restricted profile. A workload-specific exception belongs outside the agent runtime and requires explicit operating evidence. |
| OpenID Connect and OAuth 2.0 | Installation-owned identity, agent service principals, audience-bound access tokens, explicit scopes, and short-lived delegated authority. |
| [Model Context Protocol](../mcp/contract/DESIGN.md) | Protocol version `2025-11-25` under Veoveo hosted-server contract revision 2. Every capability remains typed, discoverable, policy-checked, and attributable. |
| JSON Schema 2020-12 | Closed schemas for controlled agent manifests, tool inputs, tool outputs, gateway configuration, extension fragments, and installation bindings. |
| [`veoveo.io/gateway-binding/v1`](../mcp/composer/DESIGN.md) | Installation-owned capability exposure, authorization policy, tenant binding, artifact audiences, and data-label requirements. |
| [Work Context governance](WORK_CONTEXT_GOVERNANCE.md) | Durable invocation authority, output ownership, membership, classification, data labels, and retained provenance. |
| OCI, Helm, Kubernetes, and [`veoveo.io/deployment-lock/v3`](ENTERPRISE_DEPLOYMENT.md) | Digest-addressed software, installation-owned desired state, and immutable evidence for the selected runtime closure. |

## Security Objective

A containment breach occurs when an agent, model, retrieved input, extension, or
confused deputy causes a read, write, disclosure, execution, or network effect outside
the authority resolved for that invocation. A rejected attempt is evidence of a
working boundary, not a breach.

The harness treats these inputs as untrusted:

- model output, reasoning, generated code, and tool arguments;
- prompts, operator messages, retrieved documents, recordings, and memory;
- tool results, external provider responses, web content, and connector data;
- extension servers and MCP Apps until their exact release and exposure are admitted.

The trusted computing base contains the installation's Kubernetes control plane,
nodes, container runtime, identity provider, secret manager, gateway signing boundary,
policy configuration, reconciliation controller, and verified release artifacts. An
organization that does not share those trust assumptions must use a separate cluster
or installation. Namespace and application policy do not defend against a malicious
cluster administrator, hypervisor, kernel, or physical operator.

Five properties define the target posture:

1. Authority is assigned outside the model and cannot be expanded through prompting.
2. Every operational effect crosses an authenticated, typed, policy-checked boundary.
3. Compute, time, spend, network, storage, and concurrency have enforced limits.
4. Every durable effect retains actor, provenance, policy, and output authority.
5. Operators can isolate a capability or external effect without depending on agent
   cooperation or stopping autonomous cognition.

## Continuous Operation

Agents remain autonomous throughout normal operation, dependency loss, policy denial,
and recovery. The scheduler remains enabled, accepts durable wakes, and runs every
episode whose required cognition path is available. Each capability independently
verifies its prerequisites at the moment of use. Missing identity, policy, clearance,
network, data, hardware, budget, or provider authority closes that path while the agent
remains free to observe, reason, record the denial, and pursue work that still lies
inside its envelope. Durable wakes survive a temporary cognition-provider outage.

Containment proof belongs to one exact runtime tuple: installation revision, agent
manifest and prompt, model and provider, gateway policy revision, MCP catalog,
extension releases, cluster security baseline, and data classification. A changed
tuple produces fresh proof without introducing a global autonomy switch.

## Shared Responsibility Matrix

“Product” names the mechanisms and contracts Veoveo must deliver. “Installation” names
the controls and decisions owned by the organization operating the harness. A product
mechanism without installation configuration is not an effective control. An
installation procedure cannot replace a missing product enforcement point.

### Governance And Authority

| Area | Product responsibility | Installation responsibility | Operating proof |
|---|---|---|---|
| System boundary | Publish component, trust, data-flow, and external-service boundaries. Expose the selected runtime graph through typed deployment material. | Identify the cluster, networks, providers, users, data stores, physical systems, and administrators inside the authorization boundary. | Approved boundary diagram and asset inventory match the rendered release. |
| Mission owner | Retain Work Context ownership and invocation provenance with durable work. | Assign a named business owner who accepts the purpose, operating envelope, and consequences of autonomous action. | Owner approval names the exact agent and Work Context. |
| Risk classification | Provide scopes, action targets, data labels, assurance requirements, and policy decisions. | Classify use cases by safety, privacy, security, financial, legal, and operational consequence. | Risk register maps each consequence to a preventive or recovery control. |
| Action inventory | Make every callable tool, resource, task mode, and external dependency discoverable from the active catalog. | Review the resolved catalog and prohibit capabilities not required by the mission. | Catalog diff and approved allowlist contain no unexplained surface. |
| Human authority | Support elicitation, access review, administrative policy, and durable task cancellation through governed paths. | Define actions that carry standing autonomous authority and actions that require a distinct external authorization, including its owner and lifetime. | Positive and negative authorization tests exercise consequential actions while unrelated autonomous work continues. |
| Separation of duties | Keep release publication, installation policy, runtime identity, and audit projection as distinct responsibilities. | Separate policy approval, secret custody, release promotion, incident response, and audit review where risk requires it. | Access review proves no unauthorized single principal controls every layer. |
| Exception control | Fail closed when required claims, policies, dependencies, or hardware are absent. | Time-bound every exception, identify its owner, record compensating controls, and remove it at expiry. | Exception register is empty or every entry is current and tested. |
| Containment objective | Keep authority, data, network, compute, spend, and side effects bounded outside model reasoning. | Define the installation boundary, the impact allowed inside it, and the response for any attempted escape. | Red-team evidence covers every identified path across the boundary. |

### Identity And Capability Control

| Area | Product responsibility | Installation responsibility | Operating proof |
|---|---|---|---|
| Human identity | Verify pinned OIDC issuers, signatures, audiences, nonce, PKCE, token expiry, and mapped claims. | Operate the identity provider, identity proofing, MFA, account lifecycle, group assignment, and emergency revocation. | Login, disabled-account, wrong-issuer, wrong-audience, and stale-token tests pass. |
| Agent identity | Support service principals and retain direct, delegated, or automated provenance. | Issue one non-human identity per agent or tightly coupled agent class. Never reuse an administrator, producer, or human browser credential. | Audit distinguishes every agent and its initiating human when delegated. |
| OAuth clients | Enforce registered grant, invocation mode, protected resource, scopes, and token lifetime. | Register least-privilege clients and prohibit grants that the agent does not require. | Token inspection and denied-scope tests match the approved manifest. |
| Tenant boundary | Partition governed state by the authenticated tenant before domain access. | Map each principal to one approved tenant and use separate installations when the enterprise boundary is not shared. | Cross-tenant reads, writes, tasks, artifacts, and searches are denied. |
| Work Context | Resolve membership, output policy, policy revision, classification, and data labels outside caller input. | Define context owners, members, initial grants, classifications, and lifecycle. | Viewer, contributor, custodian, owner, non-member, and stale-membership tests pass. |
| Clearance | Enforce mandatory data-label checks independently of discretionary grants. | Define label meaning and map identity-provider claims to approved clearance. | A direct grant cannot bypass a missing label; downgrade attempts fail. |
| Delegation | Preserve the effective actor, initiator, delegation identifier, and invocation mode. | Define which humans and services may delegate, to which agents, for what duration and scope. | Delegated calls remain attributable and cannot outlive their authority. |
| Gateway mediation | Route hosted capability calls through the gateway's authentication, policy, internal assertion, and audit path. | Deny agent network paths to private service endpoints that would bypass the gateway. | A direct service call fails while the equivalent authorized gateway call succeeds. |
| Capability exposure | Validate profiles, server ownership, tool/resource exposure, scopes, policy targets, and URI projection. | Bind only mission-required capabilities and review every catalog or policy revision. | Discovery returns the approved surface for the agent and nothing broader. |
| Extensions and connectors | Require typed fragments, installation bindings, conformance evidence, immutable releases, and ordinary gateway policy. | Review external data paths, credentials, provider terms, release evidence, and platform requirements before binding an extension. | Unknown, mismatched, overbroad, or colliding extensions fail composition or activation. |
| Internal trust | Sign short-lived internal assertions and distribute only public verification keys to hosted services. | Protect gateway signing keys, rotate trust deliberately, and restrict Secret access. | Key rotation, expired assertion, wrong audience, and stolen old-key tests pass. |
| Administrative authority | Keep administrative reads and mutations on explicit policy actions and the canonical owning service. | Restrict admin roles and separate agent profiles from installation administration. | The agent cannot read Secrets, change policy, register sources, or promote releases unless explicitly admitted. |
| Secret use | Consume existing Secret references and redact typed secret values from logs and debug output. | Generate independent keys, store them in an approved manager, scope projections, rotate them, and prevent prompt or memory injection. | Pod inspection, log scanning, and negative filesystem tests reveal no unauthorized secret. |

### Runtime And Network Containment

| Area | Product responsibility | Installation responsibility | Operating proof |
|---|---|---|---|
| Cluster baseline | Render non-privileged workload defaults, disabled service-account token mounts, seccomp, dropped capabilities, and explicit resources. | Harden and patch the control plane, nodes, runtime, CNI, admission controllers, and administrator access. | Cluster benchmark and rendered-manifest review cover the selected Kubernetes release. |
| Agent pod security | Keep the agent runtime compatible with the Kubernetes Restricted Pod Security profile. | Enforce the Restricted profile for the agent namespace with a pinned policy version and no broad exemption. | A violating agent pod is rejected and the running pod has no privilege escalation path. |
| Kubernetes API | Avoid mounting a Kubernetes credential unless one exact feature requires it. | Deny the agent service account API permissions. Put any inventory reader in a separate identity with read-only namespaced RBAC. | Token mount and SubjectAccessReview tests prove the agent cannot reach or mutate the API. |
| Filesystem | Use read-only roots and narrow writable volumes for declared durable state or scratch space. | Select encrypted storage, ownership, quotas, mount options, backup policy, and node-level protection. | The agent cannot write image paths, host paths, other agents' volumes, or secret projections. |
| Process execution | Put arbitrary SQL, rendering, simulation, decoding, and other executable workloads behind owning service sandboxes. | Do not grant the agent host shell, container runtime socket, privileged sidecars, host PID, host IPC, or host networking. | Escape and process-limit tests fail closed without damaging the node. |
| Default network posture | Render default-deny NetworkPolicies and declared service paths. | Install a CNI that actually enforces NetworkPolicy and validate the policy after every cluster change. | Active probes prove denied east-west and external routes are unreachable. |
| Egress | Expose explicit provider and source paths without adding a general proxy or hidden fallback. | Enumerate destination CIDRs or controlled egress proxies, DNS, ports, TLS roots, and data-use approval. Leave connected egress closed when not required. | An allowlisted endpoint succeeds; arbitrary Internet, metadata, control-plane, and private-address targets fail. |
| Outbound data | Bind provider and connector traffic to an owning service, effective principal, operation, and audit identity. | Define which classifications and fields may leave the installation through each approved destination. Apply DLP or an inspection proxy where the destination is trusted but the payload still needs control. | Seeded restricted data cannot leave through an approved endpoint used for a different purpose. |
| Ingress | Expose only declared installation routes and authenticated producer paths. | Own DNS, TLS, WAF or load balancer policy, source CIDRs, rate limits, and access-log redaction. | External scan finds only approved routes and no cluster-private service. |
| Service-to-service security | Support short-lived assertions, existing CA roots, and optional strict service-mesh mTLS. | Operate workload identity or service mesh where required and validate certificate rotation. | Wrong identity, expired certificate, plaintext downgrade, and unknown root fail. |
| SSRF and remote acquisition | Require registered sources, fixed hosts, redirect limits, media types, size limits, and elapsed-time bounds in services that acquire remote data. | Review source registrations and keep internal, link-local, metadata, and management ranges unreachable. | Redirect and address-confusion tests cannot cross the approved source boundary. |
| Compute and memory | Declare workload requests and limits and bound domain operations in typed contracts. | Apply namespace quotas, node reservations, concurrency capacity, and denial behavior under pressure. | Load tests demonstrate bounded degradation without eviction of the control and audit plane. |
| GPU isolation | Request exact Kubernetes GPU resources and fail closed without the required hardware path. | Operate the NVIDIA driver, runtime, device plugin, and capacity plan. Provide evidence for exclusive, MIG, or measured time-sliced placement. | Each one-GPU workload sees only its allocation; hardware readiness and simultaneous placement pass. |
| Browser and MCP Apps | Keep bearer tokens in the BFF, enforce CSRF, and run server-owned Apps through the sandboxed host bridge and declared dependencies. | Harden the browser fleet, origin, CSP-compatible ingress, session policy, and certificate trust. | Cross-origin, token-exfiltration, undeclared dependency, and software-renderer acceptance tests fail. |
| Analytical sandbox | Enforce bounded SQL, path, attachment, resource, and network policy in the owning DuckDB runtime. | Restrict imported data and exported destinations to governed artifact paths. | SQL cannot read host files, attach arbitrary sources, escape limits, or reach another principal's database. |

### Autonomous Execution

| Area | Product responsibility | Installation responsibility | Operating proof |
|---|---|---|---|
| Agent manifest | Validate controlled model, context, memory, episode, budget, wake, and capability configuration before execution. | Review and pin the exact manifest, preamble, migrations, and model parameters as release inputs. | Unknown fields, excessive values, and unreviewed changes are rejected while the running manifest remains authoritative. |
| Episode bound | Enforce maximum turns and rebuild each episode from bounded durable context rather than unbounded model history. | Select limits appropriate to consequence and expected work. | A non-terminating policy reaches the episode bound and stops cleanly. |
| Call budgets | Count completion and tool calls outside model reasoning and terminate the episode at configured caps. | Set per-episode and time-window budgets below the installation's risk and capacity limits. | Synthetic loops produce `budget_terminated` without an additional call. |
| Token and context budgets | Bound model output and assembled context, with deterministic truncation or rejection. | Set model-specific token ceilings and decide which context sources have priority. | Oversized memory, tool output, and wake input cannot grow a prompt without bound. |
| Financial budget | Emit usage dimensions attributable to the effective principal and operation. | Enforce provider-side spend limits, installation quotas, alerts, and an automatic cutoff independent of the model. | A low-limit exercise blocks further billed calls and identifies the responsible agent. |
| Concurrency and rate | Provide bounded queues, leases, retries, and task ownership in shared runtimes. | Set agent, tenant, provider, and capability concurrency limits that preserve the control plane. | Wake storms and parallel tool loops remain within declared rates. |
| Wake routing | Persist, deduplicate, coalesce, lease, and consume wakes through the durable runtime. | Configure wake sources, minimum intervals, and event-to-agent routing. | Duplicate events create one effective episode and stale leases recover once. |
| Durable tasks | Detach long work into typed tasks that retain authority, recovery class, progress, cancellation, and result identity. | Admit only task recovery semantics acceptable for the mission and capacity plan. | Restart and cancellation tests produce the declared terminal state without duplicate effects. |
| Retry behavior | Retry only operations whose protocol contract makes the retry safe. Provider completion remains webhook-only. | Configure provider webhook delivery, idempotency identities, and incident handling for missing completion. | Transport failure cannot duplicate a non-idempotent action or introduce provider polling. |
| Human interaction | Support bounded elicitation and durable operator messages without granting them implicit authority. | Define response timeout, authenticated responder, approval meaning, and no-response behavior. | Missing, late, spoofed, or unauthorized responses fail closed. |
| Model and provider | Keep provider choice in the reviewed manifest and keep provider credentials outside agent-visible context. | Approve model, endpoint, residency, retention, safety configuration, contractual data use, and outage behavior. | Exact model identity is observable; an unapproved endpoint or fallback cannot run. |
| Prompt and tool injection | Treat model-selected calls as untrusted requests and enforce schemas and policy after selection. | Red-team mission data, retrieved content, tool output, and operator messages for indirect instructions. | Injected content cannot expand scopes, choose another tenant, disclose secrets, or bypass approval. |
| Memory integrity | Separate durable operational state, analytical memory, and decision evidence with bounded read/write tools. | Define trusted memory sources, correction authority, retention, and poisoning review. | One agent cannot alter another agent's memory; hostile memory remains unable to change authority. |
| Result verification | Return typed outputs, immutable resource identities, provenance, and validation state where the domain defines it. | Decide which outputs require independent verification before physical, financial, legal, or release effects. | Invalid, stale, uncertain, or unverified output cannot cross its consequence gate. |
| Physical actuation | Keep physical effects behind typed domain capabilities, authenticated policy, durable task identity, and explicit operating constraints. | Own the independent safety controller, interlocks, geofences, rate and travel limits, emergency stop, maintenance mode, and human safety case. Never make model cooperation the final physical safeguard. | Commands outside the physical envelope are rejected by an independent control layer while valid autonomous commands continue. |
| Emergency capability isolation | Let operators revoke identity, remove a capability, cancel a task, or close a network path without model cooperation. Keep the agent scheduler independent from those effect paths. | Maintain authenticated isolation procedures, an on-call owner, credential rotation, and an external actuator fallback. | A drill removes the targeted effect while the agent continues bounded episodes and every in-flight task remains accounted for. |

### Data, Outputs, And Evidence

| Area | Product responsibility | Installation responsibility | Operating proof |
|---|---|---|---|
| Data minimization | Expose bounded query surfaces and avoid making chat history the system of record. | Admit only sources and fields required by the mission. Remove credentials and unrelated sensitive data before ingestion. | Sampled episodes contain no unapproved data class. |
| Classification | Carry classification, labels, owner, grants, and Work Context authority into durable outputs. | Define marking rules, label vocabulary, export restrictions, and declassification authority. | Output cannot reduce the classification or labels of its inputs without an approved release action. |
| Retrieval | Apply tenant, Work Context, scope, label, and resource policy to reads as well as writes. | Configure source access and test representative denied retrievals. | Prompt construction cannot retrieve cross-tenant or uncleared records. |
| Artifact lifecycle | Use opaque identities, explicit grants, release state, ranged delivery, and audited access. | Set owners, initial grants, retention, review, export, and deletion policy. | Unauthorized preview and download fail; approved output retains provenance. |
| Public sharing | Issue revocable, expiring, bounded read links only for releasable artifacts and keep raw bearers out of application audit. | Decide whether sharing is permitted, suppress bearer paths in edge logs, and monitor redemption. | Expired, revoked, exhausted, and guessed links fail without leaking the token. |
| Encryption and keys | Support TLS verification, separate key purposes, Secret references, and private object-store paths. | Provide approved transit and at-rest encryption, key custody, rotation, backup, and destruction. | Rotation and restore tests work without plaintext fallback or key reuse. |
| External providers | Keep provider calls behind owning services with explicit credentials, policies, and durable job records. | Approve data transfer, residency, retention, subprocessors, quotas, and incident notification. | Egress and provider logs reconcile with authorized operations and contain no unapproved payload. |
| Retention and deletion | Preserve typed lifecycle state and ownership needed to implement retention and governed deletion. | Set legal holds, retention periods, deletion approval, sanitization, and evidence retention. | Expiry and deletion drills remove intended data while preserving required audit evidence. |
| Backup and restore | Document canonical stores and avoid hidden state outside declared persistence. | Back up database, object, recording, configuration, and key material to the required recovery objectives. | Restore rehearsal reproduces authorization, ownership, tasks, and artifacts without widening access. |
| Decision record | Record episodes, tool calls, outcomes, usage, and durable operational effects with stable identities. | Protect the evidence sink, choose retention, review anomalies, and restrict audit administration. | A sampled action reconstructs who acted, under which policy, with what inputs and result. |

### Supply Chain, Operations, And Response

| Area | Product responsibility | Installation responsibility | Operating proof |
|---|---|---|---|
| Immutable release | Publish digest-addressed images, charts, schemas, compatibility material, and deployment-lock contracts. | Pin reviewed digests in desired state and prohibit mutable production resolution. | Running image IDs and rendered chart content match the approved release. |
| Provenance and SBOM | Produce source identity, checksums, SBOMs, and release evidence through the supported publication path. | Verify signatures and hashes, retain evidence, enforce registry immutability, and approve exceptions. | Verification succeeds from a clean environment before promotion. |
| Vulnerability management | Keep dependencies current when touched and publish fixed releases through the same immutable path. | Scan artifacts and nodes, monitor advisories, define severity SLAs, patch the cluster, and execute emergency promotion. | No unaccepted finding exceeds the installation's threshold or SLA. |
| Extension release | Publish conformance tooling and typed contracts for independently owned servers. | Require extension image, chart, fragment, compatibility selection, conformance report, domain smoke, and provenance. | A clean external release passes composition, render, conformance, and least-privilege review. |
| Configuration integrity | Validate gateway control data, server bootstrap documents, Helm values, and deterministic composition inputs. | Keep desired state in reviewed Git, separate secret bytes, and protect reconciliation credentials. | Drift is reconciled or alerted; an unreviewed policy change cannot reach the cluster. |
| Readiness and health | Fail readiness when mandatory dependencies, GPU paths, storage, trust material, or protocol contracts are unavailable. | Configure disruption, capacity, maintenance, and escalation around declared singleton and RWO boundaries. | Dependency-loss drills remove the workload from service without selecting an unsafe fallback. |
| Audit export | Emit identity-attributed policy, task, artifact, agent, and administration evidence. | Operate the SIEM or WORM sink, alerts, retention, access review, and time synchronization. | End-to-end audit export preserves identity, order, timestamp quality, and tamper controls. |
| Usage monitoring | Attribute provider and capability usage to principals and operations. | Set thresholds for spend, call volume, data volume, denial spikes, and unusual destinations. | Alerts fire during a controlled anomaly and identify the containment owner. |
| Clock quality | Provide Time MCP clock assessment and retain timestamps on durable evidence. | Operate approved time synchronization and define maximum offset and uncertainty. | Clock-loss or excessive-offset tests block time-sensitive acceptance. |
| Incident response | Preserve durable state needed for investigation and support credential and release replacement. | Maintain contacts, severity rules, containment playbooks, provider coordination, evidence handling, and notification duties. | A tabletop covers prompt injection, credential theft, extension compromise, exfiltration, and runaway spend. |
| Recovery | Declare task recovery classes and canonical persistence boundaries. | Define recovery time and point objectives, spare capacity, replacement credentials, and safe re-entry for each capability. | A recovery drill restores capability service without duplicating effects or widening the continuously running agent's authority. |
| Acceptance testing | Supply typed conformance, smoke, policy, GPU, browser, and deployment checks. | Run applicable tests against the real installation with production identity, networking, storage, and hardware. | Evidence is tied to the runtime tuple and contains no software-rendering or API-only substitute for visual workflows. |
| Continuous reassessment | Keep contract and release identities explicit so changes are detectable. | Repeat testing after material change and continuously monitor the containment signals owned by the installation. | Current evidence and monitoring cover the running tuple without pausing autonomous scheduling. |

## Continuous Containment Proof

The following end-to-end proofs run against the autonomous installation and demonstrate
that freedom inside the harness does not create a path outside it:

1. The agent's effective catalog contains only the approved tools, resources, prompts,
   tasks, and cross-server dependencies.
2. Wrong-tenant, wrong-context, missing-scope, missing-label, expired-token, and direct
   private-service requests are denied.
3. Arbitrary Internet, cloud metadata, cluster API, node, container-runtime, and
   unapproved provider destinations are unreachable from the agent runtime.
4. Prompt, retrieval, memory, operator-message, and tool-result injection cannot expand
   authority or disclose a seeded canary secret.
5. Turn, completion, tool-call, token, time-window, concurrency, spend, and storage
   limits terminate synthetic runaway behavior outside model cooperation.
6. Restart, duplicate delivery, stale lease, provider delay, and task cancellation
   preserve the declared idempotency and recovery result.
7. Every sampled action links the agent, initiator when delegated, tenant, Work Context,
   policy revision, capability, task, artifact or resource, usage, and outcome.
8. Emergency isolation removes a targeted identity, capability, actuator, or network
   path and accounts for in-flight work while autonomous scheduling continues.
9. Backup restoration does not revive expired credentials, stale policy, or pending
   effects outside their recovery contract.
10. The installation owner retains the exact evidence bundle beside the running tuple.

A failed capability proof closes that capability's effect path and raises an operating
incident. The agent continues autonomous cognition and may use every path that still
passes its own checks. Loss of authentication, policy, tenant isolation, mandatory
clearance, gateway mediation, network containment, or budget authority therefore
produces a denial at the affected boundary instead of a global agent stop.

## Reassessment Triggers

Fresh containment proof is produced after any change to:

- the agent manifest, preamble, memory schema, context queries, wake routing, or budget;
- the model, provider, endpoint, model parameters, or provider data-use terms;
- gateway policy, Work Context membership, identity claim mapping, scopes, or labels;
- the MCP catalog, server implementation, extension, connector, or external source;
- a container image, Helm chart, deployment lock, cluster minor version, CNI, runtime,
  GPU stack, browser, service mesh, ingress, or egress path;
- data classification, physical actuator authority, legal obligation, or mission risk;
- a containment incident, unexplained denial bypass, audit gap, or recovery event.

Routine credential rotation does not require a complete functional recertification when
the key purpose and trust contract are unchanged, but rotation tests and evidence must
pass before the old credential is retired.

## Boundary Clarifications

- Model alignment, system prompts, and refusals are useful behavior controls. They are
  not security boundaries.
- Containers and namespaces reduce blast radius. They do not protect against a trusted
  control-plane, node, kernel, hypervisor, or physical administrator compromise.
- OAuth scopes authorize functions. Tenant, Work Context, clearance, ownership, and
  network controls provide separate data and effect boundaries.
- Audit makes conduct attributable and supports detection. It does not prevent the
  action being recorded.
- Schema-valid output can still be factually wrong. Consequential domains retain their
  independent verification and approval gates.
- Continuous monitoring and repeatable tests keep containment evidence aligned with the
  exact runtime tuple as it changes.

The [regulated-work gap analysis](REGULATED_READINESS.md) tracks broader assurance work
that may be required by a particular installation. Regulatory authorization belongs to
the operating organization and is not created by this harness contract.
