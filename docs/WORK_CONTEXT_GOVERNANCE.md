# Work Context governance

A Work Context is the durable business boundary for related tasks, recordings,
agents, and artifacts. It answers four questions at the point work begins:

- Which tenant and body of work authorize this invocation?
- Who is acting, and how did that actor receive authority?
- Who owns outputs, and which principals or groups receive initial access?
- Which policy revision, classification, and data labels govern the result?

The gateway resolves those answers from authenticated identity and the active
control-plane revision. It signs the resolved authority into a short-lived
internal assertion. Hosted services consume that assertion and persist its
typed authority with durable work; callers do not supply ownership or
provenance fields.

## Control-plane model

Every installation declares its Work Contexts in the gateway control plane:

```json
{
  "work_contexts": [
    {
      "id": "flight-operations",
      "tenant": "enterprise",
      "title": "Flight operations",
      "policy_revision": "2026-07-20",
      "output_policy": {
        "owner": {
          "kind": "group",
          "id": "flight-operations"
        },
        "initial_grants": [
          {
            "subject": {
              "kind": "group",
              "id": "mission-review"
            },
            "level": "read"
          }
        ],
        "classification": "controlled",
        "data_labels": ["flight-data"]
      },
      "memberships": [
        {
          "level": "owner",
          "groups": ["flight-leads"]
        },
        {
          "level": "custodian",
          "roles": ["administrator"]
        },
        {
          "level": "contributor",
          "groups": ["flight-operators"],
          "oauth_clients": ["recording-producer"]
        },
        {
          "level": "viewer",
          "groups": ["mission-review"]
        }
      ]
    }
  ]
}
```

Membership selectors accept canonical principal IDs, identity-provider group
IDs, installation-defined role values, and OAuth client IDs. A rule matches
when one populated selector identifies the caller. The highest matching level
wins:

| Membership | Work authority | Artifact access from context membership |
|---|---|---|
| `viewer` | inspect context work | `read` |
| `contributor` | create and update context work | `write` |
| `custodian` | govern context work and review access requests | `admin` |
| `owner` | own context policy and outputs | `admin` |

Each OAuth client names one `default_work_context` and one
`invocation_mode`. Interactive clients normally use `direct`. Service clients
normally use `automated`. A delegated client supplies authenticated delegation
provenance accepted by the gateway.

## Invocation authority

An invocation has one actor and one provenance form:

| Mode | Actor | Retained provenance |
|---|---|---|
| `direct` | authenticated human principal | the actor as initiator |
| `delegated` | service or agent performing the operation | authenticated initiator and delegation ID |
| `automated` | authenticated service principal | service actor and automated mode |

The gateway verifies that the token's invocation mode matches the registered
OAuth client. It then resolves tenant, Work Context membership, output policy,
and policy revision. The resulting `InvocationAuthority` travels in the
gateway-signed internal token.

Tasks retain the authority at creation. Artifact write capabilities retain the
same authority for asynchronous completion. Recordings, agents, and artifact
occurrences store the Work Context, actor, initiator when present, invocation
mode, delegation ID when present, output policy, and policy revision. This
keeps provenance stable across task detachment, webhook completion, recording
forwarding, and agent wake cycles.

## Output ownership and access

The Work Context output policy determines the owner and initial discretionary
grants. Artifact creation stamps those values from trusted authority and unions
the context classification and data labels with the sensitivity declared by
the producing operation.

Artifact access composes three independent checks:

1. The caller and artifact belong to the same tenant.
2. Work Context membership or a direct/group grant provides the requested
   `read`, `write`, or `admin` level.
3. The caller's clearance contains every data label on the artifact.

All three checks must pass. A group grant is capped by the caller's role within
that group. A discretionary grant changes need-to-know access and never changes
tenant isolation or mandatory clearance.

The Console presents this calculation as effective access. It identifies the
sources that contributed access, the requested level, and a precise denial
reason. Artifact details also show the output owner and retained invocation
provenance.

## Access requests

A same-tenant caller whose clearance satisfies the artifact labels can request
access when need-to-know is the remaining denial. The request carries a
business justification and a requested level.

The lifecycle is:

```text
pending -> approved
        -> denied
        -> cancelled
```

The requester can cancel a pending request. A Work Context custodian or owner
can inspect its review queue. The artifact service requires effective artifact
`admin` authority for the final decision. Approval creates or replaces the
requester's direct grant and records the decision in one SurrealDB transaction.
Every transition emits audit evidence.

Tenant and clearance denials are not requestable because an artifact grant
does not alter those controls.

## Enterprise identity mapping

The starter vocabulary is deliberately small:

- `operator` identifies ordinary interactive or service use.
- `administrator` identifies installation administration.
- `operator:use` and `admin:manage` are functional OAuth scopes.

An installation maps its own directory groups and application roles to these
values in identity-provider and Work Context configuration. Microsoft Entra,
Okta, Keycloak, and other providers retain their native group IDs and
assignment workflows. Domain-specific roles such as mission lead or compliance
reviewer belong in Work Context membership rules; they do not become protocol
constants.

Use stable directory IDs for group selectors. Treat an identity-provider
application-role UUID as part of the role identity. When replacing a role
value, migrate assignments to a newly created role definition and require a
fresh login so newly issued tokens carry the canonical claim.

## Hard-cut rollout

Work Context authority is required on tasks, recordings, agents, artifact
capabilities, and artifact occurrences. Installations adopting this schema
start with a fresh platform dataset and object store. The rollout sequence is:

1. Back up data needed for external retention or audit obligations.
2. Stop producers and asynchronous workers.
3. Clear the platform database, recording data, artifact object data, and
   durable producer queues as one coordinated reset.
4. Install the chart with the new control plane and let installation bootstrap
   apply every migration.
5. Reissue browser sessions and service credentials against the configured
   Work Context.
6. Run an end-to-end producer acceptance that proves provenance, authorized
   preview, and unauthorized denial.

Migration files are immutable after a release. A development cluster carrying
an earlier hash for an unreleased migration is reset before the next bootstrap.
The complete k3d example can use its profile delete/create recipes, which also
guarantee that database and object-store state begin at the same boundary.

## Adoption checklist

- Define at least one Work Context for every configured tenant.
- Give every OAuth client a default Work Context and invocation mode.
- Map directory principals, groups, roles, and service clients to explicit
  membership levels.
- Choose a business owner for context outputs.
- Define initial grants, classification, and data labels at the context level.
- Verify that gateway signing keys are available only to the gateway.
- Test direct, delegated where used, and automated provenance.
- Test effective access for a context member, an explicitly granted principal,
  a clearance failure, and a different tenant.
- Exercise request, approval, denial, cancellation, and audit evidence.
- Prove the resulting artifact through the same Console preview and download
  paths used by operators.
