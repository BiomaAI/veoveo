-- Durable public upload sessions and bounded cross-replica accounting.
DEFINE FUNCTION fn::artifact_upload_profile_digest($profile_key: string) -> option<string> {
  LET $active = SELECT * FROM ONLY gateway_control_active:current;
  LET $profiles = SELECT VALUE document FROM gateway_control_object
    WHERE revision = $active.revision AND object_kind = 'profile' AND object_id = $profile_key;
  IF array::len($profiles) != 1 { RETURN NONE; };
  LET $profile = $profiles[0];
  LET $policies = SELECT VALUE document FROM gateway_control_object
    WHERE revision = $active.revision AND object_kind = 'policy' AND object_id = $profile.policy_version;
  IF array::len($policies) != 1 { RETURN NONE; };
  RETURN crypto::sha256(<string> [$profile, $policies[0]]);
};
DEFINE FUNCTION fn::artifact_upload_authority_matches($upload: object) -> bool {
  LET $context = SELECT * FROM ONLY $upload.work_context;
  RETURN $context != NONE AND $context.tenant = $upload.tenant
    AND $context.policy_revision = $upload.authority.policy_revision
    AND crypto::sha256(<string> [$context.policy_revision, $context.memberships, $context.output_policy]) = $upload.context_digest
    AND fn::artifact_upload_profile_digest($upload.profile_key) = $upload.profile_policy_digest;
};
DEFINE TABLE IF NOT EXISTS artifact_upload SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE artifact_upload TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS actor ON TABLE artifact_upload TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE artifact_upload TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS artifact ON TABLE artifact_upload TYPE record<artifact_occurrence>;
DEFINE FIELD IF NOT EXISTS request_id ON TABLE artifact_upload TYPE uuid;
DEFINE FIELD IF NOT EXISTS tenant_key ON TABLE artifact_upload TYPE string;
DEFINE FIELD IF NOT EXISTS actor_key ON TABLE artifact_upload TYPE string;
DEFINE FIELD IF NOT EXISTS actor_issuer ON TABLE artifact_upload TYPE string;
DEFINE FIELD IF NOT EXISTS actor_subject ON TABLE artifact_upload TYPE string;
DEFINE FIELD IF NOT EXISTS profile_key ON TABLE artifact_upload TYPE string;
DEFINE FIELD IF NOT EXISTS object_key ON TABLE artifact_upload TYPE string;
DEFINE FIELD IF NOT EXISTS actor_kind ON TABLE artifact_upload TYPE "user" | "service";
DEFINE FIELD IF NOT EXISTS context_digest ON TABLE artifact_upload TYPE string
  ASSERT string::len($value) = 64;
DEFINE FIELD IF NOT EXISTS policy_digest ON TABLE artifact_upload TYPE string
  ASSERT string::len($value) = 64;
DEFINE FIELD IF NOT EXISTS profile_policy_digest ON TABLE artifact_upload TYPE string
  ASSERT string::len($value) = 64;
DEFINE FIELD IF NOT EXISTS descriptor ON TABLE artifact_upload TYPE object;
DEFINE FIELD IF NOT EXISTS layout ON TABLE artifact_upload TYPE object;
DEFINE FIELD IF NOT EXISTS descriptor.filename ON TABLE artifact_upload TYPE string
  ASSERT string::len($value) > 0
  AND string::len($value) <= 255;
DEFINE FIELD IF NOT EXISTS descriptor.mime_type ON TABLE artifact_upload TYPE string;
DEFINE FIELD IF NOT EXISTS descriptor.byte_len ON TABLE artifact_upload TYPE option<int>
  ASSERT $value = NONE
  OR (
    $value >= 0
    AND $value <= 9007199254740991
  );
DEFINE FIELD IF NOT EXISTS descriptor.sha256 ON TABLE artifact_upload TYPE option<string>
  ASSERT $value = NONE
  OR string::len($value) = 64;
DEFINE FIELD IF NOT EXISTS layout.part_bytes ON TABLE artifact_upload TYPE int
  ASSERT $value > 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS layout.max_parts ON TABLE artifact_upload TYPE int
  ASSERT $value > 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS layout.max_total_bytes ON TABLE artifact_upload TYPE int
  ASSERT $value > 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS layout.parallel_parts ON TABLE artifact_upload TYPE int
  ASSERT $value > 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS state ON TABLE artifact_upload TYPE "open" | "finalizing" | "verifying" | "completed" | "cancelled" | "expired" | "failed";
DEFINE FIELD IF NOT EXISTS reserved_bytes ON TABLE artifact_upload TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS accepted_bytes ON TABLE artifact_upload TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS accepted_part_count ON TABLE artifact_upload TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS generation ON TABLE artifact_upload TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS cleanup_bytes ON TABLE artifact_upload TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS multipart_id ON TABLE artifact_upload TYPE option<string>;
DEFINE FIELD IF NOT EXISTS lease_owner ON TABLE artifact_upload TYPE option<uuid>;
DEFINE FIELD IF NOT EXISTS lease_until ON TABLE artifact_upload TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS manifest ON TABLE artifact_upload TYPE option<object>;
DEFINE FIELD IF NOT EXISTS manifest.byte_len ON TABLE artifact_upload TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS manifest.part_count ON TABLE artifact_upload TYPE int
  ASSERT $value > 0
  AND $value <= 10000;
DEFINE FIELD IF NOT EXISTS manifest.sha256 ON TABLE artifact_upload TYPE option<string>;
DEFINE FIELD IF NOT EXISTS verified_sha256 ON TABLE artifact_upload TYPE option<string>;
DEFINE FIELD IF NOT EXISTS completed_at ON TABLE artifact_upload TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS failure ON TABLE artifact_upload TYPE option<"integrity" | "authority_changed" | "policy_changed" | "storage" | "expired">;
DEFINE FIELD IF NOT EXISTS cleanup_pending ON TABLE artifact_upload TYPE bool;
DEFINE FIELD IF NOT EXISTS inactivity_seconds ON TABLE artifact_upload TYPE int
  ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE artifact_upload TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE artifact_upload TYPE datetime;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE artifact_upload TYPE datetime;
DEFINE FIELD IF NOT EXISTS lifetime_ends_at ON TABLE artifact_upload TYPE datetime;
DEFINE FIELD IF NOT EXISTS authority ON TABLE artifact_upload TYPE object;
DEFINE FIELD IF NOT EXISTS authority.context_key ON TABLE artifact_upload TYPE string;
DEFINE FIELD IF NOT EXISTS authority.membership ON TABLE artifact_upload TYPE "viewer" | "contributor" | "custodian" | "owner";
DEFINE FIELD IF NOT EXISTS authority.policy_revision ON TABLE artifact_upload TYPE string;
DEFINE FIELD IF NOT EXISTS authority.owner_kind ON TABLE artifact_upload TYPE "principal" | "group";
DEFINE FIELD IF NOT EXISTS authority.owner_key ON TABLE artifact_upload TYPE string;
DEFINE FIELD IF NOT EXISTS authority.initial_grants ON TABLE artifact_upload TYPE array<object> DEFAULT [];
DEFINE FIELD IF NOT EXISTS authority.initial_grants.*.subject_kind ON TABLE artifact_upload TYPE "principal" | "group";
DEFINE FIELD IF NOT EXISTS authority.initial_grants.*.subject_key ON TABLE artifact_upload TYPE string;
DEFINE FIELD IF NOT EXISTS authority.initial_grants.*.permission ON TABLE artifact_upload TYPE "read" | "write" | "admin";
DEFINE FIELD IF NOT EXISTS authority.classification ON TABLE artifact_upload TYPE option<string>;
DEFINE FIELD IF NOT EXISTS authority.data_labels ON TABLE artifact_upload TYPE array<string> DEFAULT [];
DEFINE FIELD IF NOT EXISTS authority.invocation_mode ON TABLE artifact_upload TYPE "direct" | "delegated" | "automated";
DEFINE FIELD IF NOT EXISTS authority.initiator_key ON TABLE artifact_upload TYPE option<string>;
DEFINE FIELD IF NOT EXISTS authority.delegation_id ON TABLE artifact_upload TYPE option<string>;

DEFINE INDEX IF NOT EXISTS artifact_upload_request ON artifact_upload FIELDS tenant, actor, profile_key, work_context, request_id
  UNIQUE;
DEFINE INDEX IF NOT EXISTS artifact_upload_recovery ON artifact_upload FIELDS state, lease_until;
DEFINE INDEX IF NOT EXISTS artifact_upload_expiry ON artifact_upload FIELDS expires_at;
DEFINE INDEX IF NOT EXISTS artifact_upload_cleanup ON artifact_upload FIELDS cleanup_pending, lease_until;
DEFINE TABLE IF NOT EXISTS artifact_upload_part SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS upload ON TABLE artifact_upload_part TYPE record<artifact_upload>;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE artifact_upload_part TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS part_number ON TABLE artifact_upload_part TYPE int
  ASSERT $value > 0
  AND $value <= 10000;
DEFINE FIELD IF NOT EXISTS byte_len ON TABLE artifact_upload_part TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS sha256 ON TABLE artifact_upload_part TYPE string
  ASSERT string::len($value) = 64;
DEFINE FIELD IF NOT EXISTS state ON TABLE artifact_upload_part TYPE "reserved" | "accepted";
DEFINE FIELD IF NOT EXISTS content_id ON TABLE artifact_upload_part TYPE option<string>;
DEFINE FIELD IF NOT EXISTS generation ON TABLE artifact_upload_part TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS lease_owner ON TABLE artifact_upload_part TYPE option<uuid>;
DEFINE FIELD IF NOT EXISTS lease_until ON TABLE artifact_upload_part TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE artifact_upload_part TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE artifact_upload_part TYPE datetime;
DEFINE INDEX IF NOT EXISTS artifact_upload_part_number ON artifact_upload_part FIELDS upload, part_number
  UNIQUE;
DEFINE INDEX IF NOT EXISTS artifact_upload_part_lease ON artifact_upload_part FIELDS lease_until;
DEFINE TABLE IF NOT EXISTS artifact_storage_usage SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE artifact_storage_usage TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS committed_bytes ON TABLE artifact_storage_usage TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS reserved_bytes ON TABLE artifact_storage_usage TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS cleanup_bytes ON TABLE artifact_storage_usage TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS active_uploads ON TABLE artifact_storage_usage TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS inflight_bytes ON TABLE artifact_storage_usage TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE FIELD IF NOT EXISTS inflight_parts ON TABLE artifact_storage_usage TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
DEFINE INDEX IF NOT EXISTS artifact_storage_usage_tenant ON artifact_storage_usage FIELDS tenant
  UNIQUE;
DEFINE TABLE IF NOT EXISTS artifact_upload_memory SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS inflight_bytes ON TABLE artifact_upload_memory TYPE int
  ASSERT $value >= 0
  AND $value <= 9007199254740991;
INSERT IGNORE INTO artifact_upload_memory {
  id: artifact_upload_memory:global,
  inflight_bytes: 0,
};
LET $upload_tenants = (SELECT VALUE id FROM tenant) ?? [];
IF array::len($upload_tenants) > 0 {
FOR $tenant IN $upload_tenants {
  LET $used = (SELECT VALUE math::sum(byte_len) FROM artifact_blob WHERE tenant = $tenant GROUP ALL)[0] ?? 0;
  INSERT IGNORE INTO artifact_storage_usage {
    id: type::record('artifact_storage_usage', record::id($tenant)), tenant: $tenant,
    committed_bytes: $used, reserved_bytes: 0, cleanup_bytes: 0,
    active_uploads: 0, inflight_bytes: 0, inflight_parts: 0
  };
};
};
