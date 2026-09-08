DEFINE TABLE IF NOT EXISTS artifact_read_capability SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE artifact_read_capability TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS actor ON TABLE artifact_read_capability TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE artifact_read_capability TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS context_digest ON TABLE artifact_read_capability TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD IF NOT EXISTS tenant_key ON TABLE artifact_read_capability TYPE string;
DEFINE FIELD IF NOT EXISTS actor_key ON TABLE artifact_read_capability TYPE string;
DEFINE FIELD IF NOT EXISTS actor_kind ON TABLE artifact_read_capability TYPE "user" | "service";
DEFINE FIELD IF NOT EXISTS actor_issuer ON TABLE artifact_read_capability TYPE string;
DEFINE FIELD IF NOT EXISTS actor_subject ON TABLE artifact_read_capability TYPE string;
DEFINE FIELD IF NOT EXISTS profile_key ON TABLE artifact_read_capability TYPE string;
DEFINE FIELD IF NOT EXISTS server_key ON TABLE artifact_read_capability TYPE string;
DEFINE FIELD IF NOT EXISTS task_id ON TABLE artifact_read_capability TYPE uuid;
DEFINE FIELD IF NOT EXISTS token_hash ON TABLE artifact_read_capability TYPE string
  ASSERT $value.len() = 64;
DEFINE FIELD IF NOT EXISTS labels ON TABLE artifact_read_capability TYPE array<string>
  DEFAULT [];
DEFINE FIELD IF NOT EXISTS memberships ON TABLE artifact_read_capability TYPE array<object>
  DEFAULT [];
DEFINE FIELD IF NOT EXISTS memberships.*.group_key ON TABLE artifact_read_capability TYPE string;
DEFINE FIELD IF NOT EXISTS memberships.*.permission ON TABLE artifact_read_capability TYPE "read" | "write" | "admin";
DEFINE FIELD IF NOT EXISTS max_artifact_count ON TABLE artifact_read_capability TYPE int
  ASSERT $value > 0
  AND $value <= 10000;
DEFINE FIELD IF NOT EXISTS max_total_bytes ON TABLE artifact_read_capability TYPE int
  ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS used_total_bytes ON TABLE artifact_read_capability TYPE int
  DEFAULT 0
  ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS admitted_artifacts ON TABLE artifact_read_capability TYPE array<object>
  DEFAULT [];
DEFINE FIELD IF NOT EXISTS admitted_artifacts.*.artifact ON TABLE artifact_read_capability TYPE record<artifact_occurrence>;
DEFINE FIELD IF NOT EXISTS admitted_artifacts.*.byte_len ON TABLE artifact_read_capability TYPE int
  ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE artifact_read_capability TYPE datetime;
DEFINE FIELD IF NOT EXISTS revoked_at ON TABLE artifact_read_capability TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE artifact_read_capability TYPE datetime
  DEFAULT time::now();
DEFINE FIELD IF NOT EXISTS authority ON TABLE artifact_read_capability TYPE object;
DEFINE FIELD IF NOT EXISTS authority.context_key ON TABLE artifact_read_capability TYPE string;
DEFINE FIELD IF NOT EXISTS authority.membership ON TABLE artifact_read_capability TYPE "viewer" | "contributor" | "custodian" | "owner";
DEFINE FIELD IF NOT EXISTS authority.policy_revision ON TABLE artifact_read_capability TYPE string;
DEFINE FIELD IF NOT EXISTS authority.owner_kind ON TABLE artifact_read_capability TYPE "principal" | "group";
DEFINE FIELD IF NOT EXISTS authority.owner_key ON TABLE artifact_read_capability TYPE string;
DEFINE FIELD IF NOT EXISTS authority.initial_grants ON TABLE artifact_read_capability TYPE array<object> DEFAULT [];
DEFINE FIELD IF NOT EXISTS authority.initial_grants.*.subject_kind ON TABLE artifact_read_capability TYPE "principal" | "group";
DEFINE FIELD IF NOT EXISTS authority.initial_grants.*.subject_key ON TABLE artifact_read_capability TYPE string;
DEFINE FIELD IF NOT EXISTS authority.initial_grants.*.permission ON TABLE artifact_read_capability TYPE "read" | "write" | "admin";
DEFINE FIELD IF NOT EXISTS authority.classification ON TABLE artifact_read_capability TYPE option<string>;
DEFINE FIELD IF NOT EXISTS authority.data_labels ON TABLE artifact_read_capability TYPE array<string> DEFAULT [];
DEFINE FIELD IF NOT EXISTS authority.invocation_mode ON TABLE artifact_read_capability TYPE "direct" | "delegated" | "automated";
DEFINE FIELD IF NOT EXISTS authority.initiator_key ON TABLE artifact_read_capability TYPE option<string>;
DEFINE FIELD IF NOT EXISTS authority.delegation_id ON TABLE artifact_read_capability TYPE option<string>;
DEFINE INDEX IF NOT EXISTS artifact_read_capability_token_unique ON TABLE artifact_read_capability FIELDS token_hash
  UNIQUE;
DEFINE INDEX IF NOT EXISTS artifact_read_capability_task ON TABLE artifact_read_capability FIELDS tenant, server_key, task_id;
DEFINE INDEX IF NOT EXISTS artifact_read_capability_expiry ON TABLE artifact_read_capability FIELDS expires_at, revoked_at;
