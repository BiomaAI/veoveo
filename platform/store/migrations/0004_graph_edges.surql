-- Metadata-bearing relationships. Simple ownership remains a typed record link.

DEFINE TABLE IF NOT EXISTS membership SCHEMAFULL TYPE RELATION IN principal OUT principal_group ENFORCED CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE membership TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS role ON TABLE membership TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE membership TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS membership_endpoints_unique ON TABLE membership FIELDS in, out UNIQUE;
DEFINE INDEX IF NOT EXISTS membership_group ON TABLE membership FIELDS out, role;

DEFINE TABLE IF NOT EXISTS artifact_grant SCHEMAFULL TYPE RELATION IN artifact_occurrence OUT principal | principal_group ENFORCED CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE artifact_grant TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS permission ON TABLE artifact_grant TYPE "read" | "write" | "admin";
DEFINE FIELD IF NOT EXISTS subject_kind ON TABLE artifact_grant TYPE "user" | "group";
DEFINE FIELD IF NOT EXISTS subject_key ON TABLE artifact_grant TYPE string;
DEFINE FIELD IF NOT EXISTS labels ON TABLE artifact_grant TYPE array<string> DEFAULT [];
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE artifact_grant TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS created_by ON TABLE artifact_grant TYPE record<principal>;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE artifact_grant TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS artifact_grant_endpoints_unique ON TABLE artifact_grant FIELDS in, out UNIQUE;
DEFINE INDEX IF NOT EXISTS artifact_grant_subject_expiry ON TABLE artifact_grant FIELDS out, expires_at;

DEFINE TABLE IF NOT EXISTS profile_server SCHEMAFULL TYPE RELATION IN profile OUT mcp_server ENFORCED CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE profile_server TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS namespace ON TABLE profile_server TYPE string;
DEFINE FIELD IF NOT EXISTS enabled ON TABLE profile_server TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE profile_server TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS profile_server_endpoints_unique ON TABLE profile_server FIELDS in, out UNIQUE;
DEFINE INDEX IF NOT EXISTS profile_server_namespace_unique ON TABLE profile_server FIELDS in, namespace UNIQUE;

DEFINE TABLE IF NOT EXISTS task_produced_artifact SCHEMAFULL TYPE RELATION IN task OUT artifact_occurrence ENFORCED CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE task_produced_artifact TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS name ON TABLE task_produced_artifact TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE task_produced_artifact TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS task_produced_artifact_unique ON TABLE task_produced_artifact FIELDS in, name UNIQUE;
DEFINE INDEX IF NOT EXISTS artifact_producer ON TABLE task_produced_artifact FIELDS out;

DEFINE TABLE IF NOT EXISTS task_used_artifact SCHEMAFULL TYPE RELATION IN task OUT artifact_occurrence ENFORCED CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE task_used_artifact TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS name ON TABLE task_used_artifact TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE task_used_artifact TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS task_used_artifact_unique ON TABLE task_used_artifact FIELDS in, out, name UNIQUE;
DEFINE INDEX IF NOT EXISTS artifact_consumers ON TABLE task_used_artifact FIELDS out;

DEFINE TABLE IF NOT EXISTS artifact_derived_from SCHEMAFULL TYPE RELATION IN artifact_occurrence OUT artifact_occurrence ENFORCED CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE artifact_derived_from TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS relation ON TABLE artifact_derived_from TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE artifact_derived_from TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS artifact_derivation_unique ON TABLE artifact_derived_from FIELDS in, out, relation UNIQUE;
DEFINE INDEX IF NOT EXISTS artifact_derivation_parent ON TABLE artifact_derived_from FIELDS out;

DEFINE TABLE IF NOT EXISTS task_used_frame SCHEMAFULL TYPE RELATION IN task OUT frame ENFORCED CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE task_used_frame TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS name ON TABLE task_used_frame TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE task_used_frame TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS task_used_frame_unique ON TABLE task_used_frame FIELDS in, out, name UNIQUE;
DEFINE INDEX IF NOT EXISTS frame_task_use ON TABLE task_used_frame FIELDS out;

DEFINE TABLE IF NOT EXISTS recording_segment SCHEMAFULL TYPE RELATION IN recording OUT segment ENFORCED CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE recording_segment TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS ordinal ON TABLE recording_segment TYPE int ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE recording_segment TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS recording_segment_endpoint_unique ON TABLE recording_segment FIELDS in, out UNIQUE;
DEFINE INDEX IF NOT EXISTS recording_segment_ordinal_unique ON TABLE recording_segment FIELDS in, ordinal UNIQUE;

DEFINE TABLE IF NOT EXISTS agent_owner SCHEMAFULL TYPE RELATION IN agent OUT principal | principal_group ENFORCED CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE agent_owner TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS role ON TABLE agent_owner TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE agent_owner TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS agent_owner_endpoints_unique ON TABLE agent_owner FIELDS in, out UNIQUE;
DEFINE INDEX IF NOT EXISTS agent_owner_subject ON TABLE agent_owner FIELDS out, role;
