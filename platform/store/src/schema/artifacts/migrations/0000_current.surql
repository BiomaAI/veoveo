-- Current owner schema and fresh bootstrap state.

DEFINE TABLE artifact_access_request TYPE NORMAL SCHEMAFULL CHANGEFEED 12w6d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE artifact_blob TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE artifact_derived_from TYPE RELATION IN artifact_occurrence OUT artifact_occurrence ENFORCED SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE artifact_grant TYPE RELATION IN artifact_occurrence OUT principal | principal_group ENFORCED SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE artifact_occurrence TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE artifact_read_capability TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE artifact_storage_usage TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE artifact_upload TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE artifact_upload_memory TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE artifact_upload_part TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE artifact_write_capability TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE artifact_write_redemption TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE share_link TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE FUNCTION fn::artifact_upload_authority_matches($upload: object) -> bool {
    LET $context_id: record<work_context> = $upload.work_context;
    LET $context = (SELECT * FROM ONLY $context_id);
    IF type::is_object($context) THEN {
        LET $authority: object = $upload.authority;
        RETURN $context.tenant = $upload.tenant
            AND $context.policy_revision = $authority.policy_revision
            AND crypto::sha256(<string> [$context.policy_revision, $context.memberships, $context.output_policy]) = $upload.context_digest
            AND fn::artifact_upload_profile_digest($upload.profile_key) = $upload.profile_policy_digest;
    } ELSE { RETURN false } END;
} PERMISSIONS FULL;

DEFINE FUNCTION fn::artifact_upload_profile_digest($profile_key: string) -> none | string {
    LET $active = (SELECT * FROM ONLY gateway_control_active:current);
    IF type::is_object($active) THEN {
        LET $profiles = (SELECT VALUE document FROM gateway_control_object
            WHERE revision = $active.revision AND object_kind = 'profile' AND object_id = $profile_key);
        IF array::len($profiles) != 1 { RETURN NONE };
        LET $profile: object = array::first($profiles);
        LET $policies = (SELECT VALUE document FROM gateway_control_object
            WHERE revision = $active.revision AND object_kind = 'policy' AND object_id = $profile.policy_version);
        IF array::len($policies) != 1 { RETURN NONE };
        LET $policy: object = array::first($policies);
        RETURN crypto::sha256(<string> [$profile, $policy]);
    } ELSE { RETURN NONE } END;
} PERMISSIONS FULL;

DEFINE FIELD OVERWRITE artifact ON artifact_access_request TYPE record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON artifact_access_request TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE decided_at ON artifact_access_request TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE decided_by ON artifact_access_request TYPE none | record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE decided_by_key ON artifact_access_request TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE decision_note ON artifact_access_request TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON artifact_access_request TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE justification ON artifact_access_request TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE requested_level ON artifact_access_request TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE requester ON artifact_access_request TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE requester_key ON artifact_access_request TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON artifact_access_request TYPE int DEFAULT 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON artifact_access_request TYPE 'pending' | 'approved' | 'denied' | 'cancelled' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON artifact_access_request TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON artifact_access_request TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON artifact_access_request TYPE record<work_context> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context_key ON artifact_access_request TYPE string PERMISSIONS FULL;

DEFINE INDEX artifact_access_request_artifact_state ON artifact_access_request FIELDS artifact, state, created_at;

DEFINE INDEX artifact_access_request_requester_state ON artifact_access_request FIELDS tenant, requester, state, created_at;

DEFINE INDEX artifact_access_request_subject_unique ON artifact_access_request FIELDS tenant, artifact, requester UNIQUE;

DEFINE FIELD OVERWRITE byte_len ON artifact_blob TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE content_type ON artifact_blob TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON artifact_blob TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE encryption ON artifact_blob TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON artifact_blob TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE object_key ON artifact_blob TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE sha256 ON artifact_blob TYPE string ASSERT $value.len() = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON artifact_blob TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX artifact_blob_object_key_unique ON artifact_blob FIELDS object_key UNIQUE;

DEFINE INDEX artifact_blob_tenant_sha_unique ON artifact_blob FIELDS tenant, sha256 UNIQUE;

DEFINE FIELD OVERWRITE created_at ON artifact_derived_from TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON artifact_derived_from TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE in ON artifact_derived_from TYPE record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE out ON artifact_derived_from TYPE record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE relation ON artifact_derived_from TYPE string PERMISSIONS FULL;

DEFINE INDEX artifact_derivation_parent ON artifact_derived_from FIELDS out;

DEFINE INDEX artifact_derivation_unique ON artifact_derived_from FIELDS in, out, relation UNIQUE;

DEFINE FIELD OVERWRITE created_at ON artifact_grant TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by ON artifact_grant TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON artifact_grant TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON artifact_grant TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE in ON artifact_grant TYPE record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels ON artifact_grant TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels.* ON artifact_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE out ON artifact_grant TYPE record<principal | principal_group> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE permission ON artifact_grant TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE subject_key ON artifact_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE subject_kind ON artifact_grant TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE INDEX artifact_grant_endpoints_unique ON artifact_grant FIELDS in, out UNIQUE;

DEFINE INDEX artifact_grant_external_subject ON artifact_grant FIELDS in, subject_kind, subject_key UNIQUE;

DEFINE INDEX artifact_grant_subject_expiry ON artifact_grant FIELDS out, expires_at;

DEFINE FIELD OVERWRITE authority ON artifact_occurrence TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON artifact_occurrence TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON artifact_occurrence TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON artifact_occurrence TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON artifact_occurrence TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON artifact_occurrence TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON artifact_occurrence TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON artifact_occurrence TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON artifact_occurrence TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON artifact_occurrence TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON artifact_occurrence TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON artifact_occurrence TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON artifact_occurrence TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON artifact_occurrence TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON artifact_occurrence TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON artifact_occurrence TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON artifact_occurrence TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE blob ON artifact_occurrence TYPE record<artifact_blob> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE classification ON artifact_occurrence TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON artifact_occurrence TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE delegation_id ON artifact_occurrence TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE filename ON artifact_occurrence TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON artifact_occurrence TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE initiator ON artifact_occurrence TYPE none | record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE initiator_key ON artifact_occurrence TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE invocation_mode ON artifact_occurrence TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels ON artifact_occurrence TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels.* ON artifact_occurrence TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE media_type ON artifact_occurrence TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE metadata ON artifact_occurrence TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON artifact_occurrence TYPE record<principal> | record<principal_group> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_key ON artifact_occurrence TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_kind ON artifact_occurrence TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE policy_revision ON artifact_occurrence TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE producer ON artifact_occurrence TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE producer_key ON artifact_occurrence TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE release_state ON artifact_occurrence TYPE 'private' | 'releasable' | 'released' DEFAULT 'private' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE retention_expires_at ON artifact_occurrence TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE search_text ON artifact_occurrence TYPE string DEFAULT '' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON artifact_occurrence TYPE none | record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON artifact_occurrence TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON artifact_occurrence TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON artifact_occurrence TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX artifact_occurrence_blob ON artifact_occurrence FIELDS blob;

DEFINE INDEX artifact_occurrence_context_created ON artifact_occurrence FIELDS tenant, work_context, created_at;

DEFINE INDEX artifact_occurrence_count ON artifact_occurrence COUNT;

DEFINE INDEX artifact_occurrence_owner_created ON artifact_occurrence FIELDS tenant, owner, created_at;

DEFINE INDEX artifact_occurrence_producer_created ON artifact_occurrence FIELDS tenant, producer, created_at;

DEFINE INDEX artifact_occurrence_release ON artifact_occurrence FIELDS tenant, release_state;

DEFINE INDEX artifact_occurrence_retention ON artifact_occurrence FIELDS retention_expires_at;

DEFINE INDEX artifact_occurrence_search ON artifact_occurrence FIELDS search_text FULLTEXT ANALYZER platform_search BM25(1.2,0.75);

DEFINE INDEX artifact_occurrence_task ON artifact_occurrence FIELDS task;

DEFINE FIELD OVERWRITE actor ON artifact_read_capability TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_issuer ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_key ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_kind ON artifact_read_capability TYPE 'user' | 'service' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_subject ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE admitted_artifacts ON artifact_read_capability TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE admitted_artifacts.* ON artifact_read_capability TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE admitted_artifacts.*.artifact ON artifact_read_capability TYPE record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE admitted_artifacts.*.byte_len ON artifact_read_capability TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE audit ON artifact_read_capability TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON artifact_read_capability TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON artifact_read_capability TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON artifact_read_capability TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON artifact_read_capability TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON artifact_read_capability TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON artifact_read_capability TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON artifact_read_capability TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON artifact_read_capability TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON artifact_read_capability TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON artifact_read_capability TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON artifact_read_capability TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON artifact_read_capability TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE context_digest ON artifact_read_capability TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON artifact_read_capability TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON artifact_read_capability TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels ON artifact_read_capability TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels.* ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE max_artifact_count ON artifact_read_capability TYPE int ASSERT $value > 0 AND $value <= 10000 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE max_total_bytes ON artifact_read_capability TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships ON artifact_read_capability TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.* ON artifact_read_capability TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.*.group_key ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memberships.*.permission ON artifact_read_capability TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile_key ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revoked_at ON artifact_read_capability TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE server_key ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_id ON artifact_read_capability TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON artifact_read_capability TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant_key ON artifact_read_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE token_hash ON artifact_read_capability TYPE string ASSERT $value.len() = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE used_total_bytes ON artifact_read_capability TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON artifact_read_capability TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX artifact_read_capability_expiry ON artifact_read_capability FIELDS expires_at, revoked_at;

DEFINE INDEX artifact_read_capability_task ON artifact_read_capability FIELDS tenant, server_key, task_id;

DEFINE INDEX artifact_read_capability_token_unique ON artifact_read_capability FIELDS token_hash UNIQUE;

DEFINE FIELD OVERWRITE active_uploads ON artifact_storage_usage TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE cleanup_bytes ON artifact_storage_usage TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE committed_bytes ON artifact_storage_usage TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE inflight_bytes ON artifact_storage_usage TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE inflight_parts ON artifact_storage_usage TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE reserved_bytes ON artifact_storage_usage TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON artifact_storage_usage TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX artifact_storage_usage_tenant ON artifact_storage_usage FIELDS tenant UNIQUE;

DEFINE FIELD OVERWRITE accepted_bytes ON artifact_upload TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE accepted_part_count ON artifact_upload TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor ON artifact_upload TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_issuer ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_key ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_kind ON artifact_upload TYPE 'user' | 'service' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_subject ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE artifact ON artifact_upload TYPE record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE audit ON artifact_upload TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON artifact_upload TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON artifact_upload TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON artifact_upload TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON artifact_upload TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON artifact_upload TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON artifact_upload TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON artifact_upload TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON artifact_upload TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON artifact_upload TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON artifact_upload TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON artifact_upload TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON artifact_upload TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE cleanup_bytes ON artifact_upload TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE cleanup_pending ON artifact_upload TYPE bool PERMISSIONS FULL;

DEFINE FIELD OVERWRITE completed_at ON artifact_upload TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE context_digest ON artifact_upload TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON artifact_upload TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE descriptor ON artifact_upload TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE descriptor.byte_len ON artifact_upload TYPE none | int ASSERT $value = NONE OR $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE descriptor.filename ON artifact_upload TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 255 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE descriptor.mime_type ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE descriptor.sha256 ON artifact_upload TYPE none | string ASSERT $value = NONE OR string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON artifact_upload TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE failure ON artifact_upload TYPE none | 'integrity' | 'authority_changed' | 'policy_changed' | 'storage' | 'expired' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE generation ON artifact_upload TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE inactivity_seconds ON artifact_upload TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layout ON artifact_upload TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layout.max_parts ON artifact_upload TYPE int ASSERT $value > 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layout.max_total_bytes ON artifact_upload TYPE int ASSERT $value > 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layout.parallel_parts ON artifact_upload TYPE int ASSERT $value > 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE layout.part_bytes ON artifact_upload TYPE int ASSERT $value > 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_owner ON artifact_upload TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_until ON artifact_upload TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lifetime_ends_at ON artifact_upload TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE manifest ON artifact_upload TYPE none | object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE manifest.byte_len ON artifact_upload TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE manifest.part_count ON artifact_upload TYPE int ASSERT $value > 0 AND $value <= 10000 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE manifest.sha256 ON artifact_upload TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE multipart_id ON artifact_upload TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE object_key ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE policy_digest ON artifact_upload TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile_key ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile_policy_digest ON artifact_upload TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE request_id ON artifact_upload TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE reserved_bytes ON artifact_upload TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON artifact_upload TYPE 'open' | 'finalizing' | 'verifying' | 'completed' | 'cancelled' | 'expired' | 'failed' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON artifact_upload TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant_key ON artifact_upload TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON artifact_upload TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE verified_sha256 ON artifact_upload TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON artifact_upload TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX artifact_upload_cleanup ON artifact_upload FIELDS cleanup_pending, lease_until;

DEFINE INDEX artifact_upload_expiry ON artifact_upload FIELDS expires_at;

DEFINE INDEX artifact_upload_recovery ON artifact_upload FIELDS state, lease_until;

DEFINE INDEX artifact_upload_request ON artifact_upload FIELDS tenant, actor, profile_key, work_context, request_id UNIQUE;

DEFINE FIELD OVERWRITE inflight_bytes ON artifact_upload_memory TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE byte_len ON artifact_upload_part TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE content_id ON artifact_upload_part TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON artifact_upload_part TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE generation ON artifact_upload_part TYPE int ASSERT $value >= 0 AND $value <= 9007199254740991 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_owner ON artifact_upload_part TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_until ON artifact_upload_part TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE part_number ON artifact_upload_part TYPE int ASSERT $value > 0 AND $value <= 10000 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE sha256 ON artifact_upload_part TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON artifact_upload_part TYPE 'reserved' | 'accepted' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON artifact_upload_part TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON artifact_upload_part TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE upload ON artifact_upload_part TYPE record<artifact_upload> PERMISSIONS FULL;

DEFINE INDEX artifact_upload_part_lease ON artifact_upload_part FIELDS lease_until;

DEFINE INDEX artifact_upload_part_number ON artifact_upload_part FIELDS upload, part_number UNIQUE;

DEFINE FIELD OVERWRITE actor ON artifact_write_capability TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_issuer ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_key ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_kind ON artifact_write_capability TYPE 'user' | 'service' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_subject ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE audit ON artifact_write_capability TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON artifact_write_capability TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON artifact_write_capability TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON artifact_write_capability TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON artifact_write_capability TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON artifact_write_capability TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON artifact_write_capability TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON artifact_write_capability TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON artifact_write_capability TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON artifact_write_capability TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON artifact_write_capability TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON artifact_write_capability TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON artifact_write_capability TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON artifact_write_capability TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON artifact_write_capability TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON artifact_write_capability TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels ON artifact_write_capability TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE labels.* ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE max_artifact_count ON artifact_write_capability TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE max_total_bytes ON artifact_write_capability TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile_key ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revoked_at ON artifact_write_capability TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE server_key ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_id ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON artifact_write_capability TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant_key ON artifact_write_capability TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE token_hash ON artifact_write_capability TYPE string ASSERT $value.len() = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE used_artifact_count ON artifact_write_capability TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE used_total_bytes ON artifact_write_capability TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON artifact_write_capability TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX artifact_write_capability_expiry ON artifact_write_capability FIELDS expires_at, revoked_at;

DEFINE INDEX artifact_write_capability_task ON artifact_write_capability FIELDS tenant, server_key, task_id;

DEFINE INDEX artifact_write_capability_token_hash_unique ON artifact_write_capability FIELDS token_hash UNIQUE;

DEFINE FIELD OVERWRITE artifact ON artifact_write_redemption TYPE record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE byte_len ON artifact_write_redemption TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE capability ON artifact_write_redemption TYPE record<artifact_write_capability> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE finalized_at ON artifact_write_redemption TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON artifact_write_redemption TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE idempotency_key ON artifact_write_redemption TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE request_hash ON artifact_write_redemption TYPE string ASSERT $value.len() = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE reserved_at ON artifact_write_redemption TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON artifact_write_redemption TYPE 'reserved' | 'finalized' DEFAULT 'reserved' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON artifact_write_redemption TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_id ON artifact_write_redemption TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON artifact_write_redemption TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX artifact_write_redemption_artifact ON artifact_write_redemption FIELDS artifact UNIQUE;

DEFINE INDEX artifact_write_redemption_capability_key ON artifact_write_redemption FIELDS capability, idempotency_key UNIQUE;

DEFINE INDEX artifact_write_redemption_state ON artifact_write_redemption FIELDS state, reserved_at;

DEFINE FIELD OVERWRITE artifact ON share_link TYPE record<artifact_occurrence> REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON share_link TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by ON share_link TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE download_count ON share_link TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON share_link TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON share_link TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE max_downloads ON share_link TYPE none | int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE permission ON share_link TYPE 'read' DEFAULT 'read' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revoked_at ON share_link TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON share_link TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE token_hash ON share_link TYPE string PERMISSIONS FULL;

DEFINE INDEX share_link_active_artifact ON share_link FIELDS artifact, revoked_at, expires_at;

DEFINE INDEX share_link_artifact ON share_link FIELDS artifact, created_at;

DEFINE INDEX share_link_count ON share_link COUNT;

DEFINE INDEX share_link_expiry_revocation ON share_link FIELDS expires_at, revoked_at;

DEFINE INDEX share_link_token_hash_unique ON share_link FIELDS token_hash UNIQUE;

-- Initial state for a fresh owner lane.
CREATE ONLY artifact_upload_memory:global SET inflight_bytes = 0;
