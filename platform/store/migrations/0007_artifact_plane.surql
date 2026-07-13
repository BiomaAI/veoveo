-- Artifact-plane security state layered over the occurrence/blob schema.

-- Preserve the external authorization subject alongside the referential link.
-- Record ids remain internal platform identities; policy decisions use these
-- strongly typed user/group keys when reconstructing the public grant model.
DEFINE FIELD IF NOT EXISTS subject_kind ON TABLE artifact_grant TYPE "user" | "group";
DEFINE FIELD IF NOT EXISTS subject_key ON TABLE artifact_grant TYPE string;
DEFINE INDEX IF NOT EXISTS artifact_grant_external_subject ON TABLE artifact_grant FIELDS in, subject_kind, subject_key UNIQUE;

DEFINE TABLE IF NOT EXISTS artifact_write_capability SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE artifact_write_capability TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE artifact_write_capability TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS owner ON TABLE artifact_write_capability TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS tenant_key ON TABLE artifact_write_capability TYPE string;
DEFINE FIELD IF NOT EXISTS owner_key ON TABLE artifact_write_capability TYPE string;
DEFINE FIELD IF NOT EXISTS owner_kind ON TABLE artifact_write_capability TYPE "user" | "service";
DEFINE FIELD IF NOT EXISTS owner_issuer ON TABLE artifact_write_capability TYPE string;
DEFINE FIELD IF NOT EXISTS owner_subject ON TABLE artifact_write_capability TYPE string;
DEFINE FIELD IF NOT EXISTS profile_key ON TABLE artifact_write_capability TYPE string;
DEFINE FIELD IF NOT EXISTS server_key ON TABLE artifact_write_capability TYPE string;
DEFINE FIELD IF NOT EXISTS task_id ON TABLE artifact_write_capability TYPE string;
DEFINE FIELD IF NOT EXISTS token_hash ON TABLE artifact_write_capability TYPE string ASSERT $value.len() = 64;
DEFINE FIELD IF NOT EXISTS labels ON TABLE artifact_write_capability TYPE array<string> DEFAULT [];
DEFINE FIELD IF NOT EXISTS max_artifact_count ON TABLE artifact_write_capability TYPE int ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS max_total_bytes ON TABLE artifact_write_capability TYPE int ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS used_artifact_count ON TABLE artifact_write_capability TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS used_total_bytes ON TABLE artifact_write_capability TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE artifact_write_capability TYPE datetime;
DEFINE FIELD IF NOT EXISTS revoked_at ON TABLE artifact_write_capability TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE artifact_write_capability TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS artifact_write_capability_token_hash_unique ON TABLE artifact_write_capability FIELDS token_hash UNIQUE;
DEFINE INDEX IF NOT EXISTS artifact_write_capability_task ON TABLE artifact_write_capability FIELDS tenant, server_key, task_id;
DEFINE INDEX IF NOT EXISTS artifact_write_capability_expiry ON TABLE artifact_write_capability FIELDS expires_at, revoked_at;

-- Public links are bearer read capabilities only. Counts and revocation remain
-- durable even after the artifact bytes are collected.
DEFINE FIELD OVERWRITE permission ON TABLE share_link TYPE "read" DEFAULT "read";
DEFINE INDEX IF NOT EXISTS share_link_active_artifact ON TABLE share_link FIELDS artifact, revoked_at, expires_at;
