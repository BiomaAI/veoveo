DEFINE TABLE IF NOT EXISTS simulation_view_state SCHEMAFULL TYPE NORMAL
  CHANGEFEED 90d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE simulation_view_state TYPE string;
DEFINE FIELD IF NOT EXISTS tenant_key ON TABLE simulation_view_state TYPE string;
DEFINE FIELD IF NOT EXISTS owner_key ON TABLE simulation_view_state TYPE string;
DEFINE FIELD IF NOT EXISTS work_context_key ON TABLE simulation_view_state TYPE string;
DEFINE FIELD IF NOT EXISTS policy_revision ON TABLE simulation_view_state TYPE string;
DEFINE FIELD IF NOT EXISTS session_id ON TABLE simulation_view_state TYPE string;
DEFINE FIELD IF NOT EXISTS epoch_id ON TABLE simulation_view_state TYPE string;
DEFINE FIELD IF NOT EXISTS desired_revision ON TABLE simulation_view_state TYPE int
  ASSERT $value > 0;
DEFINE FIELD IF NOT EXISTS realized_revision ON TABLE simulation_view_state TYPE int
  ASSERT $value >= 0
  AND $value <= desired_revision;
DEFINE FIELD IF NOT EXISTS authorization_revision ON TABLE simulation_view_state TYPE int
  ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS revoked ON TABLE simulation_view_state TYPE bool;
DEFINE FIELD IF NOT EXISTS authorization_expires_at ON TABLE simulation_view_state TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS snapshot_digest ON TABLE simulation_view_state TYPE string
  ASSERT $value.len() = 64;
DEFINE FIELD IF NOT EXISTS snapshot ON TABLE simulation_view_state TYPE object FLEXIBLE;
DEFINE FIELD IF NOT EXISTS reconciliation ON TABLE simulation_view_state TYPE object FLEXIBLE;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE simulation_view_state TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE simulation_view_state TYPE datetime;
DEFINE INDEX IF NOT EXISTS simulation_view_state_scope_session_unique ON TABLE simulation_view_state FIELDS tenant_key, work_context_key, session_id
  UNIQUE;
DEFINE INDEX IF NOT EXISTS simulation_view_state_reconcile ON TABLE simulation_view_state FIELDS revoked, desired_revision, realized_revision, updated_at;
DEFINE INDEX IF NOT EXISTS simulation_view_state_authorization_expiry ON TABLE simulation_view_state FIELDS revoked, authorization_expires_at;
