-- Immutable gateway control-plane publications and their atomic active pointer.

DEFINE TABLE IF NOT EXISTS gateway_control_revision SCHEMAFULL TYPE NORMAL CHANGEFEED 90d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE gateway_control_revision TYPE string;
DEFINE FIELD IF NOT EXISTS revision_id ON TABLE gateway_control_revision TYPE string READONLY;
DEFINE FIELD IF NOT EXISTS sha256 ON TABLE gateway_control_revision TYPE string READONLY ASSERT $value.len() = 64;
DEFINE FIELD IF NOT EXISTS source ON TABLE gateway_control_revision TYPE "admin_api" | "seed_file" READONLY;
DEFINE FIELD IF NOT EXISTS applied_at ON TABLE gateway_control_revision TYPE datetime READONLY;
DEFINE FIELD IF NOT EXISTS applied_by ON TABLE gateway_control_revision TYPE string READONLY;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE gateway_control_revision TYPE option<string> READONLY;
DEFINE FIELD IF NOT EXISTS control_plane ON TABLE gateway_control_revision TYPE object FLEXIBLE READONLY;
DEFINE INDEX IF NOT EXISTS gateway_control_revision_id_unique ON TABLE gateway_control_revision FIELDS revision_id UNIQUE;
DEFINE INDEX IF NOT EXISTS gateway_control_revision_sha256 ON TABLE gateway_control_revision FIELDS sha256;
DEFINE INDEX IF NOT EXISTS gateway_control_revision_applied ON TABLE gateway_control_revision FIELDS applied_at, revision_id;

DEFINE TABLE IF NOT EXISTS gateway_control_object SCHEMAFULL TYPE NORMAL CHANGEFEED 90d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE gateway_control_object TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS revision ON TABLE gateway_control_object TYPE record<gateway_control_revision> READONLY;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE gateway_control_object TYPE option<string> READONLY;
DEFINE FIELD IF NOT EXISTS object_kind ON TABLE gateway_control_object TYPE string READONLY;
DEFINE FIELD IF NOT EXISTS object_id ON TABLE gateway_control_object TYPE string READONLY;
DEFINE FIELD IF NOT EXISTS document ON TABLE gateway_control_object TYPE object FLEXIBLE READONLY;
DEFINE INDEX IF NOT EXISTS gateway_control_object_identity_unique ON TABLE gateway_control_object FIELDS revision, object_kind, object_id UNIQUE;
DEFINE INDEX IF NOT EXISTS gateway_control_object_kind_id ON TABLE gateway_control_object FIELDS object_kind, object_id;
DEFINE INDEX IF NOT EXISTS gateway_control_object_tenant_kind ON TABLE gateway_control_object FIELDS tenant, object_kind;
DEFINE INDEX IF NOT EXISTS gateway_control_object_revision ON TABLE gateway_control_object FIELDS revision;

DEFINE TABLE IF NOT EXISTS gateway_control_active SCHEMAFULL TYPE NORMAL CHANGEFEED 90d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE gateway_control_active TYPE string;
DEFINE FIELD IF NOT EXISTS revision ON TABLE gateway_control_active TYPE record<gateway_control_revision>;
DEFINE FIELD IF NOT EXISTS revision_id ON TABLE gateway_control_active TYPE string;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE gateway_control_active TYPE datetime;
DEFINE INDEX IF NOT EXISTS gateway_control_active_revision ON TABLE gateway_control_active FIELDS revision UNIQUE;
