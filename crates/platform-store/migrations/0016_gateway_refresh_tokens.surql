-- Rotating, replay-detecting OAuth refresh-token families.

DEFINE TABLE IF NOT EXISTS gateway_refresh_family SCHEMAFULL TYPE NORMAL CHANGEFEED 90d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE gateway_refresh_family TYPE uuid;
DEFINE FIELD IF NOT EXISTS authorization_server ON TABLE gateway_refresh_family TYPE string;
DEFINE FIELD IF NOT EXISTS profile ON TABLE gateway_refresh_family TYPE string;
DEFINE FIELD IF NOT EXISTS oauth_client_id ON TABLE gateway_refresh_family TYPE string;
DEFINE FIELD IF NOT EXISTS principal_id ON TABLE gateway_refresh_family TYPE string;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE gateway_refresh_family TYPE option<string>;
DEFINE FIELD IF NOT EXISTS scopes ON TABLE gateway_refresh_family TYPE array<string>;
DEFINE FIELD IF NOT EXISTS principal ON TABLE gateway_refresh_family TYPE object FLEXIBLE;
DEFINE FIELD IF NOT EXISTS current_generation ON TABLE gateway_refresh_family TYPE int ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS issued_at ON TABLE gateway_refresh_family TYPE datetime;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE gateway_refresh_family TYPE datetime;
DEFINE FIELD IF NOT EXISTS revoked_at ON TABLE gateway_refresh_family TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS revocation_reason ON TABLE gateway_refresh_family TYPE option<string>;
DEFINE INDEX IF NOT EXISTS gateway_refresh_family_client ON TABLE gateway_refresh_family FIELDS authorization_server, profile, oauth_client_id, principal_id, issued_at;
DEFINE INDEX IF NOT EXISTS gateway_refresh_family_expiry ON TABLE gateway_refresh_family FIELDS expires_at;

DEFINE TABLE IF NOT EXISTS gateway_refresh_token SCHEMAFULL TYPE NORMAL CHANGEFEED 90d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE gateway_refresh_token TYPE uuid;
DEFINE FIELD IF NOT EXISTS family ON TABLE gateway_refresh_token TYPE record<gateway_refresh_family> READONLY;
DEFINE FIELD IF NOT EXISTS token_hash ON TABLE gateway_refresh_token TYPE string READONLY ASSERT $value.len() = 64;
DEFINE FIELD IF NOT EXISTS generation ON TABLE gateway_refresh_token TYPE int READONLY ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS issued_at ON TABLE gateway_refresh_token TYPE datetime READONLY;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE gateway_refresh_token TYPE datetime READONLY;
DEFINE FIELD IF NOT EXISTS consumed_at ON TABLE gateway_refresh_token TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS replacement ON TABLE gateway_refresh_token TYPE option<record<gateway_refresh_token>>;
DEFINE FIELD IF NOT EXISTS replay_detected_at ON TABLE gateway_refresh_token TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS delivery_envelope ON TABLE gateway_refresh_token TYPE option<string>;
DEFINE FIELD IF NOT EXISTS delivery_expires_at ON TABLE gateway_refresh_token TYPE option<datetime>;
DEFINE INDEX IF NOT EXISTS gateway_refresh_token_hash_unique ON TABLE gateway_refresh_token FIELDS token_hash UNIQUE;
DEFINE INDEX IF NOT EXISTS gateway_refresh_token_family_generation_unique ON TABLE gateway_refresh_token FIELDS family, generation UNIQUE;
DEFINE INDEX IF NOT EXISTS gateway_refresh_token_expiry ON TABLE gateway_refresh_token FIELDS expires_at;
DEFINE INDEX IF NOT EXISTS gateway_refresh_token_delivery_expiry ON TABLE gateway_refresh_token FIELDS delivery_expires_at;
