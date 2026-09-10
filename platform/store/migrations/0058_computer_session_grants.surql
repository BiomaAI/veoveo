-- Additive private browser-grant ledger. No existing Computer or operation is rewritten.
DEFINE TABLE computer_session_grant SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD grant_id ON computer_session_grant TYPE uuid;
DEFINE FIELD computer_id ON computer_session_grant TYPE uuid;
DEFINE FIELD owner_key ON computer_session_grant TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD provider_instance_id ON computer_session_grant TYPE uuid;
DEFINE FIELD authority ON computer_session_grant TYPE object FLEXIBLE;
DEFINE FIELD family ON computer_session_grant TYPE record<gateway_refresh_family>;
DEFINE FIELD ticket_hash ON computer_session_grant TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD ticket_expires_at ON computer_session_grant TYPE datetime;
DEFINE FIELD connection_id ON computer_session_grant TYPE option<uuid>;
DEFINE FIELD issued_at ON computer_session_grant TYPE datetime;
DEFINE FIELD expires_at ON computer_session_grant TYPE datetime;
DEFINE FIELD idle_expires_at ON computer_session_grant TYPE datetime;
DEFINE FIELD last_activity_at ON computer_session_grant TYPE datetime;
DEFINE FIELD revoked_at ON computer_session_grant TYPE option<datetime>;
DEFINE FIELD provider_resource_id ON computer_session_grant TYPE string;
DEFINE FIELD process_id ON computer_session_grant TYPE string;
DEFINE INDEX computer_session_grant_id ON computer_session_grant FIELDS grant_id UNIQUE;
DEFINE INDEX computer_session_grant_page ON computer_session_grant FIELDS owner_key, computer_id, grant_id;

DEFINE TABLE computer_session_grant_guard SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD revision ON computer_session_grant_guard TYPE int ASSERT $value >= 0;

DEFINE TABLE computer_session_grant_policy SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD fingerprint ON computer_session_grant_policy TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD max_grants ON computer_session_grant_policy TYPE int ASSERT $value >= 0 AND $value <= 128;
DEFINE FIELD absolute_seconds ON computer_session_grant_policy TYPE int ASSERT $value > 0 AND $value <= 86400;
DEFINE FIELD idle_seconds ON computer_session_grant_policy TYPE int ASSERT $value > 0 AND $value <= 86400;
DEFINE FIELD admissions ON computer_session_grant_policy TYPE int ASSERT $value >= 0;
