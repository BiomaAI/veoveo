# Regulated Work Readiness

Status: exploratory gap analysis, not normative. Researched 2026-07-23 against
the current tree and primary regulatory sources. This document assesses what
Veoveo needs to credibly support PII, CUI, ITAR, and FedRAMP-shaped work
through one holistic control approach rather than per-regime features.

## Authorization Posture

The product is never the authorized thing. The customer's deployment is.
FedRAMP scopes itself to cloud services operated on behalf of an agency (OMB
M-24-15) and explicitly excludes single-agency self-operated systems. CMMC
assesses the contractor's information system, and a software vendor whose
assets never touch customer CUI is not an External Service Provider (32 CFR
170). DoD Impact Levels attach to cloud service offerings, so "runs at IL5"
is a deployment claim that self-hosted software inherits from its enclave.
ITAR registration and licensing attach to the organization. GDPR and CCPA
obligations attach to the controller. Veoveo therefore never needs its own
ATO or FedRAMP authorization while it remains customer-deployed with no
Veoveo-operated service in the data path. That changes only if a hosted
offering appears, at which point FedRAMP 20x applies (mandatory for new
entrants January 1, 2027).

The consequence is clarifying. Veoveo's job is to ship one product-side
control fabric plus one evidence package, and let each customer map both
into their own authorization. This is the pattern GitLab, Rancher
Government, Chainguard, HashiCorp, and Palantir already follow. The
per-regime work is mapping documents, not code.

Veoveo's founding architecture is unusually aligned with this posture. No
vendor control plane, no telemetry sink, no license service, and customer-
held keys are exactly what preserves the ITAR §120.54 end-to-end encryption
carve-out, under which storing and sending technical data is not an export
when secured with validated cryptography and the means of decryption reach
no third party. The self-hosted posture is not a compliance obstacle. It is
the compliance product.

## Shared Responsibility Model

Veoveo installs into an enterprise environment and inherits that
environment's institutional controls. Control origination follows the
FedRAMP/OSCAL categories: a control is **customer-provided** (the
installation owner's program or infrastructure), **customer-configured**
(product mechanism, organization-defined parameter values), **shared**, or
**product-implemented**. The customer responsibility matrix in the evidence
package will state this per control; the table below fixes the split at the
control-area level.

Identity deserves the precise statement first, in SP 800-63 terms. The
enterprise identity provider is the credential service provider. Identity
proofing (IAL), authenticator policy and strength (AAL — MFA, PIV/CAC,
phishing resistance), account lifecycle, and personnel adjudications such
as US-person determination under a Technology Control Plan are
customer-provided, asserted to the product as federation claims. Veoveo is
the relying party: its obligations are federation conformance (FAL —
pinned issuer and audience, mandatory PKCE, signed assertions), fail-closed
enforcement of required claims, and a published identity claims contract.
The reference installation (`examples/bioma`, veoveo.bioma.ai) demonstrates
this division with Microsoft Entra as the customer-provided CSP. Keycloak
exists in this repository solely as a CI test fixture and is not a product
recommendation.

| Control area | Origination | Customer provides | Product provides |
|---|---|---|---|
| Identification and authentication (IA; 800-63 IAL/AAL/FAL) | Customer-provided | CSP/IdP operation, identity proofing, authenticator policy, account lifecycle, US-person adjudication asserted as claims | Relying-party conformance, fail-closed claim enforcement, identity claims contract |
| Access enforcement (AC-2/3/6/16) | Shared | Policy rules, data labels, Work Context membership decisions | Policy engine, mandatory clearance backstop, tenant partition, per-request enforcement |
| System use notification and session control (AC-8/11/12) | Customer-configured | Banner text, timeout and lock ODP values | Banner display, session termination, lock and re-authentication mechanisms |
| Cryptographic protection (SC-8/13/28, IA-7) | Shared | Encrypted volumes, KMS, key custody, host FIPS posture | Validated modules in product binaries (FIPS mode), TLS configuration, no plaintext bypass paths |
| Boundary protection (SC-7) | Shared | Perimeter, WAF, DNS, physical security | Default-deny NetworkPolicies, enumerated egress, offline connectivity mode |
| Media and storage protection (SC-28, MP-6, AU-9(2)) | Shared | An S3 store satisfying the declared capability contract, backup operations | Storage capability contract, conformance probe, fail-closed governed features, sanitization support |
| Audit and accountability (AU-2/3/6/9/11) | Shared | WORM/SIEM retention sink, log review, retention ODP values | Complete identity-attributed capture of every decision, documented export schema |
| Security assessment and program (CA, PL, PM; ITAR 22 CFR 122; privacy program) | Customer-provided | ATO or self-assessment, Technology Control Plan, export licenses, marking policy | Evidence package: CRM, OSCAL component definition, hardening guide, marking display mechanisms |

Two duties stay product-implemented even with identity fully delegated.
Session controls and the pre-logon system use notification render in
Veoveo's surfaces, so the mechanisms are ours to build even though every
value is an organization-defined parameter. And the identity claims
contract is a product document: the exact claims a CSP must issue —
subject, tenant, groups, roles, data labels, `principal_assurances`, and
authenticator context (acr/amr) once the vocabulary exists — with per-IdP
mapping notes, Entra first.

## The Control Fabric

NIST SP 800-171 r3 is tailored directly from the SP 800-53B moderate
baseline, CMMC Level 2 pins to 800-171 r2, FedRAMP baselines are 800-53
selections, ITAR reduces to access control plus validated encryption plus
audit, and privacy regimes reduce to minimization, security, and lifecycle
rights. One 800-53-moderate-shaped capability set therefore satisfies every
lineage at once. Eight pillars organize it:

| Pillar | What the product must provide | Regimes served |
|---|---|---|
| 1. Classification and marking | Label data at creation, enforce clearance on every read, display and propagate markings | CUI, ITAR, PII |
| 2. Identity and access | Strong authn vocabulary, attribute-gated authorization, least privilege, session controls | All |
| 3. Cryptography | FIPS-validated modules for transit and rest, customer-held keys | All (ITAR §120.54 explicitly) |
| 4. Audit and evidence | Immutable, identity-attributed, exportable records of every action | All |
| 5. Data lifecycle | Retention, selective deletion, sanitization support, residency | PII (erasure), CUI (retention) |
| 6. Boundary and egress | Enumerated, deniable egress; air-gap operation; no phone-home | ITAR, CUI, FedRAMP |
| 7. Supply chain | SBOM, signed artifacts, provenance, hardened bases, pinned digests | FedRAMP SR/KSI-SCR, 800-171 r3 03.17 |
| 8. Evidence and program | CRM, OSCAL, hardening guide, vulnerability SLAs, pen test | All (assessor-facing) |

## Current Control Implementation

The as-built inventory (file evidence throughout the tree, verified this
date) supports a stronger starting claim than most products at this stage.

**Pillar 1 — largely built.** Work Contexts carry classification and data
labels; artifact creation stamps authority from the gateway-signed token and
unions context labels with declared sensitivity, so producers cannot
self-declassify (`platform/artifacts/service/src/service.rs:301-344`).
Access composes tenancy, discretionary grants, and a mandatory clearance
check that no grant can override (`mcp/contract/src/access.rs:274-293`).
Missing: UI banners and marking display, marking propagation into exports
and recordings, and a clearance re-check on public share redemption
(`service.rs:596-632` gates only on release state).

**Pillar 2 — strong core, thin vocabulary.** PKCE is mandatory S256,
symmetric JWT algorithms are structurally rejected, refresh tokens rotate in
replay-detected families, the Console BFF keeps bearer tokens out of the
browser, and policy is fail-closed ABAC with tenant partition checked first.
The assurance vocabulary has exactly one value, `UsPerson`
(`mcp/contract/src/gateway/policy.rs:195-196`) — the ITAR hook exists and is
enforced end to end, but assurance acquisition is delegated entirely to
undocumented IdP claim mapping, and there is no authenticator-strength
(AAL) vocabulary, no session lock/timeout controls, and no system use
notification (AC-8/11/12).

**Pillar 3 — clean stack, no validated boundary.** The workspace links pure
rustls with the ring provider and no OpenSSL anywhere. That cleanliness
makes the FIPS conversion tractable: rustls' `fips` feature switches to the
AWS-LC FIPS module (FIPS 140-3 certificates #4816 static, #5298/#5314 for
AWS-LC 3 with ML-KEM and Ed25519 approved). Today nothing at rest is
encrypted by the product: SurrealDB is plain RocksDB on a PVC, the default
object store (rustfs, a beta component) has no SSE settings, and the
symmetric crypto in use (XChaCha20-Poly1305 for session cookies and refresh
envelopes) is not a FIPS-approved algorithm. At-rest protection properly
originates customer-side (encrypted volumes on validated modules, KMS key
custody); the product-side work is SSE passthrough, documentation, and the
envelope-key layer that enables cryptographic erasure.

**Pillar 4 — complete capture, no immutability.** Every MCP call, auth
outcome, and admin operation lands in `audit_event` with actor, source IP,
trace ID, and reason codes. But integrity rests on database access control
alone: no WORM sink, no hash chaining, and a retention job that hard-deletes
audit records after a rolling window (`platform/store/src/gateway_runtime.rs:727-741`).
For AU-9 this is the inverted problem — evidence silently expires.

**Pillar 5 — locatable, not deletable.** Tenant-scoped blob keys, cataloged
segments, and stamped metadata make regulated data findable. Nothing makes
it selectively erasable: no deletion path exists for artifact occurrences,
blobs (`BlobStore::delete` has zero call sites), recordings, RRD segments
(append-only by design), or DuckDB owner workspaces; blob dedup has no
reference count, so even out-of-band S3 deletion is unsafe; the outbox
retains mutation history. The only documented erasure is a whole-installation
reset. No data residency control exists beyond an S3 region string.

**Pillar 6 — best-in-class posture, three leaks.** Default-deny
NetworkPolicies, an offline mode that drops all egress rules, an enumerable
provider list, and verified absence of phone-home. The leaks: the agent
kernel's LLM endpoint is Cloudflare Workers AI in the production manifest
(`agents/kernel/src/llm.rs`), so agent context leaves the boundary unless a
local endpoint is configured; the media provider receives full generation
inputs; NetworkPolicy exceptions omit link-local (169.254.0.0/16) and CGNAT
(100.64.0.0/10) ranges; and Rerun's usage analytics default on in release
builds. Separately, Google Photorealistic 3D Tiles prohibits caching,
offline use, and ML analysis outright — that content source cannot follow
the product into restricted deployments at all.

**Pillar 7 — SBOMs yes, signatures no.** The offline bundle ships SPDX
SBOMs per image, SHA-256 checksums, and digest-pinned external images. There
is no cosign/Sigstore signing, no SLSA provenance, no verification at
admission, and Veoveo's own images are tag-pinned rather than digest-pinned
in the bundle lockfile. The k3s reference is not the STIG-certified
Kubernetes (RKE2 is), and MinIO — the obvious object-store choice a year ago
— is now unmaintained community-side, which validates the rustfs caution but
demands a governed storage profile with Object Lock.

**Pillar 8 — not started.** No customer responsibility matrix, OSCAL
component definition, hardening guide, SCAP profile, CVD policy, published
patch SLAs, VEX practice, or product pen test exists. This is normal for the
product's stage and is where assessors will look first.

## Gap Register (POA&M Input)

Priority 0 blocks a regulated deal today. Priority 1 hardens the claim.
Priority 2 is the program that makes it durable.

| # | Gap | Pillar | Priority |
|---|---|---|---|
| G1 | No FIPS mode: ring provider, XChaCha20-Poly1305 envelopes, no validated module claim | 3 | 0 |
| G2 | No SSE/KMS hooks in the chart; encrypted-volume requirement and key custody undocumented | 3 | 0 |
| G3 | No selective deletion: artifacts, blobs (no refcount), recordings, workspaces | 5 | 0 |
| G4 | Audit not immutable and silently expires; no WORM export path | 4 | 0 |
| G5 | No AAL vocabulary (MFA/PIV/acr), session controls, or system use notification | 2 | 0 |
| G6 | Agent LLM egress defaults external in the production manifest | 6 | 0 |
| G7 | No image signing, provenance, or own-image digest pinning | 7 | 1 |
| G8 | Share-link redemption skips label clearance; export/marking propagation absent | 1 | 1 |
| G9 | No governed storage profile: default store is beta rustfs with no Object Lock capability | 7 | 1 |
| G10 | Restricted-deployment geo content: Google tiles licensing bars caching/offline/ML use | 6 | 1 |
| G11 | NetworkPolicy link-local/CGNAT gaps; Rerun analytics opt-out not baked | 6 | 1 |
| G12 | Secret backends: KMS/cloud/enterprise variants declared but unimplemented | 3 | 1 |
| G13 | No residency/jurisdiction pinning per tenant or Work Context | 5 | 2 |
| G14 | No CRM, OSCAL, hardening guide, SCAP, CVD/SLA, VEX, or pen test | 8 | 2 |
| G15 | CUI marking display and DoD consent banner not implemented | 1 | 2 |
| G16 | k3s reference profile has no STIG/FIPS lineage; RKE2 profile absent | 7 | 2 |

## Remediation Backlog

Work items grouped by control family. Each item carries its gap reference,
priority, and the primary 800-53 r5 controls it satisfies (800-171 r3
families follow the same lineage). P0 items block a regulated engagement;
P1 items harden the claim; P2 items build the assessor-facing program.

### Cryptographic modules (SC-8, SC-13, IA-7)

- [ ] G1/P0 — Workspace FIPS feature: rustls `fips` with the aws-lc-rs FIPS
      provider (FIPS 140-3 certs #4816/#5298/#5314), runtime assertion via
      `ClientConfig::fips()`, tested-OE statement in docs
- [ ] G1/P0 — Replace XChaCha20-Poly1305 session-cookie and refresh-envelope
      crypto with AES-256-GCM sourced from the validated module under FIPS
- [ ] G1/P0 — `cargo deny` gate banning `ring` and stray RustCrypto digest
      crates inside the security boundary; CI FIPS build (CMake and Go in
      the toolchain, musl prebuilt bindings for static images)
- [ ] G12/P1 — Implement the declared KMS-backed and cloud secret sources
      (`SecretSource` variants currently fail closed)

### Storage, media protection, and the filesystem plane (SC-28, MP-6, SI-12, AU-9(2))

- [ ] G9/P0 — Governed storage profile in the chart: SeaweedFS with Object
      Lock, versioning, and SSE-KMS as the bundled governed default; rustfs
      remains the non-governed embedded default for development and edge
- [ ] G9/P0 — Storage capability conformance probe (Object Lock, versioning,
      SSE headers) at startup or in the smoke harness; governed features
      fail closed against a store that does not satisfy the contract
- [ ] G2/P0 — SSE-KMS parameter passthrough for `externalS3` in the chart
- [ ] G2/P0 — Document the customer-provided at-rest posture: encrypted
      volumes on validated modules (LUKS/FIPS kernel or cloud KMS) and
      customer key custody as the ITAR §120.54 end-to-end condition
- [ ] G2/P1 — Enable DuckDB 1.4 native encryption as defense in depth;
      pre-stage extensions, keep auto-install disabled
- [ ] G3/P0 — Blob reference counting under tenant-scoped dedup so byte
      deletion is safe
- [ ] G3/P0 — Erasure APIs: artifact occurrence, sealed recording and its
      segments, DuckDB owner workspace; retention-driven GC honoring
      `retention_expires_at`
- [ ] G3/P1 — Per-object envelope keys enabling cryptographic erasure
      (crypto-shredding) for append-only RRD segments and customer backups
- [ ] G13/P2 — Residency/jurisdiction pinning per tenant and Work Context

### Audit and accountability (AU-4, AU-9, AU-11)

- [ ] G4/P0 — Audit export pipeline to an Object-Locked bucket in the
      governed storage profile (WORM sink)
- [ ] G4/P0 — Audit retention becomes an organization-defined parameter
      with no silent default deletion
- [ ] G4/P1 — Published SIEM-consumable audit export schema

### Identification, authentication, and session (IA-2, IA-8, AC-8, AC-11, AC-12)

- [ ] G5/P0 — Extend `PrincipalAssurance` with authenticator-context values
      mapped from acr/amr claims (AAL evidence), alongside `UsPerson`
- [ ] G5/P0 — Session termination, timeout, and lock with ODP values
- [ ] G5/P0 — Pre-logon system use notification with mandatory
      acknowledgment; re-display on session lock
- [ ] G5/P1 — Identity claims contract document with Entra reference
      mapping (subject, tenant, groups, roles, data labels,
      `principal_assurances`, acr/amr), executable by an enterprise IdP
      team without Veoveo involvement
- [ ] G5/P1 — Optional control-plane assignment of assurances to principals
      as an alternative to IdP custom claims

### Boundary protection (SC-7)

- [ ] G6/P0 — Agent kernel defaults to an in-boundary OpenAI-compatible
      inference endpoint; external endpoints require explicit opt-in
- [ ] G11/P1 — NetworkPolicy exceptions for 169.254.0.0/16 and
      100.64.0.0/10 in every external-egress rule
- [ ] G11/P1 — Rerun analytics opt-out baked into images and denied by
      NetworkPolicy
- [ ] G10/P1 — Governed geospatial content profile: Cesium ion Self-Hosted
      for 3D Tiles, Protomaps/OpenMapTiles basemaps, USGS 3DEP terrain.
      Compliance substance: tile requests disclose areas of interest to a
      commercial third party (AC-4 information flow at the SC-7 boundary),
      restricted enclaves cannot reach the streaming services at all, and
      Google's terms prohibit the caching, offline use, and ML analysis
      the platform performs on scene content — a contractual bar
      independent of security controls. Google tiles remain for connected
      commercial visualization only

### Marking and classification (AC-16; 32 CFR 2002.20; DoDI 5200.48)

- [ ] G8/P1 — Label clearance re-check or a label ceiling on public share
      redemption; marking propagation into exports
- [ ] G15/P2 — CUI banner and footer, designation indicator block, splash
      alert, and marking propagation into prints and recordings

### Supply chain and configuration (SR-3, SR-4, SR-11, CM-6)

- [ ] G7/P1 — cosign signing of images and OCI charts; SLSA provenance as
      in-toto attestations; published verification instructions
- [ ] G7/P1 — Digest-pin Veoveo's own images in the offline bundle lockfile
- [ ] G16/P2 — RKE2-based hardened reference profile beside the k3d
      developer profile
- [ ] G16/P2 — FIPS/STIG-hardened base images for product containers
- [ ] G14/P2 — Hardening guide with a SCAP-checkable profile

### Assessment evidence (CA-2, PL-2, RA-5)

- [ ] G14/P2 — Customer responsibility matrix against 800-171 r2 and r3
      plus the 800-53 moderate baseline, with control origination stated
      per control
- [ ] G14/P2 — OSCAL component definition
- [ ] G14/P2 — CVD policy, published patch SLAs benchmarked to Iron Bank
      and FedRAMP class timeframes, VEX statements per release
- [ ] G14/P2 — Independent product penetration test
- [ ] P2 — Model provenance register for shipped and customer-supplied
      weights while the BIS replacement rule is pending; track NIST COSAiS
      800-53 overlays for AI agent systems

## Timing Notes

FIPS 140-2 certificates sunset globally on September 21, 2026, so any FIPS
commitment made now should cite only 140-3 Active certificates (AWS-LC
#4816/#5298/#5314, OpenSSL provider #4985, Go module #5247). CMMC Phase II
was suspended July 13, 2026 while Phase 1 self-assessments remain in force,
which raises the near-term value of self-assessment-ready evidence. FedRAMP
20x becomes mandatory for new cloud entrants January 1, 2027 and matters
only if a hosted offering appears. OMB M-25-21 presumptively treats AI
controlling vehicle or robot movement as high-impact, which makes the
existing provenance, rehearsal, and audit surfaces a customer-side
compliance asset worth stating in those terms.
