-- Durable gateway correctness state shared by every gateway replica.

DEFINE TABLE IF NOT EXISTS gateway_resource_subscription SCHEMAFULL TYPE NORMAL CHANGEFEED 90d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE gateway_resource_subscription TYPE uuid;
DEFINE FIELD IF NOT EXISTS profile ON TABLE gateway_resource_subscription TYPE string;
DEFINE FIELD IF NOT EXISTS owner ON TABLE gateway_resource_subscription TYPE string;
DEFINE FIELD IF NOT EXISTS upstream_server ON TABLE gateway_resource_subscription TYPE string;
DEFINE FIELD IF NOT EXISTS resource_uri ON TABLE gateway_resource_subscription TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE gateway_resource_subscription TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE gateway_resource_subscription TYPE datetime;
DEFINE FIELD IF NOT EXISTS payload ON TABLE gateway_resource_subscription TYPE object FLEXIBLE;
DEFINE INDEX IF NOT EXISTS gateway_resource_subscription_identity_unique ON TABLE gateway_resource_subscription FIELDS profile, owner, upstream_server, resource_uri UNIQUE;
DEFINE INDEX IF NOT EXISTS gateway_resource_subscription_owner ON TABLE gateway_resource_subscription FIELDS profile, owner, upstream_server, updated_at;

DEFINE TABLE IF NOT EXISTS gateway_jwt_revocation SCHEMAFULL TYPE NORMAL CHANGEFEED 90d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE gateway_jwt_revocation TYPE uuid;
DEFINE FIELD IF NOT EXISTS profile ON TABLE gateway_jwt_revocation TYPE string;
DEFINE FIELD IF NOT EXISTS issuer ON TABLE gateway_jwt_revocation TYPE string;
DEFINE FIELD IF NOT EXISTS jwt_id ON TABLE gateway_jwt_revocation TYPE string;
DEFINE FIELD IF NOT EXISTS revoked_at ON TABLE gateway_jwt_revocation TYPE datetime;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE gateway_jwt_revocation TYPE datetime;
DEFINE FIELD IF NOT EXISTS reason ON TABLE gateway_jwt_revocation TYPE option<string>;
DEFINE FIELD IF NOT EXISTS payload ON TABLE gateway_jwt_revocation TYPE object FLEXIBLE;
DEFINE INDEX IF NOT EXISTS gateway_jwt_revocation_identity_unique ON TABLE gateway_jwt_revocation FIELDS profile, issuer, jwt_id UNIQUE;
DEFINE INDEX IF NOT EXISTS gateway_jwt_revocation_expiry ON TABLE gateway_jwt_revocation FIELDS expires_at;

DEFINE TABLE IF NOT EXISTS gateway_replay_id SCHEMAFULL TYPE NORMAL CHANGEFEED 90d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE gateway_replay_id TYPE uuid;
DEFINE FIELD IF NOT EXISTS kind ON TABLE gateway_replay_id TYPE "client_assertion" | "id_jag";
DEFINE FIELD IF NOT EXISTS authorization_server ON TABLE gateway_replay_id TYPE string;
DEFINE FIELD IF NOT EXISTS client_id ON TABLE gateway_replay_id TYPE string;
DEFINE FIELD IF NOT EXISTS jwt_id ON TABLE gateway_replay_id TYPE string;
DEFINE FIELD IF NOT EXISTS seen_at ON TABLE gateway_replay_id TYPE datetime;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE gateway_replay_id TYPE datetime;
DEFINE INDEX IF NOT EXISTS gateway_replay_id_identity_unique ON TABLE gateway_replay_id FIELDS kind, authorization_server, client_id, jwt_id UNIQUE;
DEFINE INDEX IF NOT EXISTS gateway_replay_id_expiry ON TABLE gateway_replay_id FIELDS kind, expires_at;

DEFINE TABLE IF NOT EXISTS gateway_authorization_request SCHEMAFULL TYPE NORMAL CHANGEFEED 90d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE gateway_authorization_request TYPE uuid;
DEFINE FIELD IF NOT EXISTS idp_state ON TABLE gateway_authorization_request TYPE string;
DEFINE FIELD IF NOT EXISTS profile ON TABLE gateway_authorization_request TYPE string;
DEFINE FIELD IF NOT EXISTS oauth_client_id ON TABLE gateway_authorization_request TYPE string;
DEFINE FIELD IF NOT EXISTS oidc_client ON TABLE gateway_authorization_request TYPE string;
DEFINE FIELD IF NOT EXISTS redirect_uri ON TABLE gateway_authorization_request TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE gateway_authorization_request TYPE datetime;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE gateway_authorization_request TYPE datetime;
DEFINE FIELD IF NOT EXISTS payload ON TABLE gateway_authorization_request TYPE object FLEXIBLE;
DEFINE INDEX IF NOT EXISTS gateway_authorization_request_state_unique ON TABLE gateway_authorization_request FIELDS idp_state UNIQUE;
DEFINE INDEX IF NOT EXISTS gateway_authorization_request_expiry ON TABLE gateway_authorization_request FIELDS expires_at;
DEFINE INDEX IF NOT EXISTS gateway_authorization_request_client ON TABLE gateway_authorization_request FIELDS profile, oauth_client_id, created_at;

DEFINE TABLE IF NOT EXISTS gateway_authorization_code SCHEMAFULL TYPE NORMAL CHANGEFEED 90d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE gateway_authorization_code TYPE uuid;
DEFINE FIELD IF NOT EXISTS code ON TABLE gateway_authorization_code TYPE string;
DEFINE FIELD IF NOT EXISTS profile ON TABLE gateway_authorization_code TYPE string;
DEFINE FIELD IF NOT EXISTS oauth_client_id ON TABLE gateway_authorization_code TYPE string;
DEFINE FIELD IF NOT EXISTS oidc_client ON TABLE gateway_authorization_code TYPE string;
DEFINE FIELD IF NOT EXISTS principal ON TABLE gateway_authorization_code TYPE string;
DEFINE FIELD IF NOT EXISTS redirect_uri ON TABLE gateway_authorization_code TYPE string;
DEFINE FIELD IF NOT EXISTS issued_at ON TABLE gateway_authorization_code TYPE datetime;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE gateway_authorization_code TYPE datetime;
DEFINE FIELD IF NOT EXISTS consumed_at ON TABLE gateway_authorization_code TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS payload ON TABLE gateway_authorization_code TYPE object FLEXIBLE;
DEFINE INDEX IF NOT EXISTS gateway_authorization_code_value_unique ON TABLE gateway_authorization_code FIELDS code UNIQUE;
DEFINE INDEX IF NOT EXISTS gateway_authorization_code_expiry ON TABLE gateway_authorization_code FIELDS expires_at;
DEFINE INDEX IF NOT EXISTS gateway_authorization_code_client ON TABLE gateway_authorization_code FIELDS profile, oauth_client_id, principal, issued_at;
