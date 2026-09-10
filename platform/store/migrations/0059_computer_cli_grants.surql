-- Additive private CLI ledger. Enable CLI admission only after every Computers
-- worker counts these grants. Rollback drains access; never remove retained homes.
DEFINE TABLE computer_cli_pairing SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD pairing_id ON computer_cli_pairing TYPE uuid;
DEFINE FIELD computer_id ON computer_cli_pairing TYPE uuid;
DEFINE FIELD owner_key ON computer_cli_pairing TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD family ON computer_cli_pairing TYPE record<gateway_refresh_family>;
DEFINE FIELD binding_hash ON computer_cli_pairing TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD code_hash ON computer_cli_pairing TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD name ON computer_cli_pairing TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 64;
DEFINE FIELD callback_port ON computer_cli_pairing TYPE int ASSERT $value >= 1024 AND $value <= 65535;
DEFINE FIELD issued_at ON computer_cli_pairing TYPE datetime;
DEFINE FIELD expires_at ON computer_cli_pairing TYPE datetime;
DEFINE FIELD consumed_at ON computer_cli_pairing TYPE option<datetime>;
DEFINE INDEX computer_cli_pairing_rate ON computer_cli_pairing FIELDS owner_key, issued_at;
DEFINE INDEX computer_cli_pairing_code ON computer_cli_pairing FIELDS owner_key, computer_id, code_hash;

DEFINE TABLE computer_cli_pairing_guard SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD revision ON computer_cli_pairing_guard TYPE int ASSERT $value >= 0;

DEFINE TABLE computer_cli_grant SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD grant_id ON computer_cli_grant TYPE uuid;
DEFINE FIELD computer_id ON computer_cli_grant TYPE uuid;
DEFINE FIELD owner_key ON computer_cli_grant TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD provider_instance_id ON computer_cli_grant TYPE uuid;
DEFINE FIELD authority ON computer_cli_grant TYPE object FLEXIBLE;
DEFINE FIELD family ON computer_cli_grant TYPE record<gateway_refresh_family>;
DEFINE FIELD credential_hash ON computer_cli_grant TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD name ON computer_cli_grant TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 64;
DEFINE FIELD issued_at ON computer_cli_grant TYPE datetime;
DEFINE FIELD expires_at ON computer_cli_grant TYPE datetime;
DEFINE FIELD idle_expires_at ON computer_cli_grant TYPE datetime;
DEFINE FIELD last_activity_at ON computer_cli_grant TYPE datetime;
DEFINE FIELD revoked_at ON computer_cli_grant TYPE option<datetime>;
DEFINE FIELD connections_revision ON computer_cli_grant TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE INDEX computer_cli_grant_id ON computer_cli_grant FIELDS grant_id UNIQUE;
DEFINE INDEX computer_cli_grant_page ON computer_cli_grant FIELDS owner_key, computer_id, grant_id;

DEFINE TABLE computer_cli_connection SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD connection_id ON computer_cli_connection TYPE uuid;
DEFINE FIELD grant_id ON computer_cli_connection TYPE uuid;
DEFINE FIELD provider_resource_id ON computer_cli_connection TYPE string;
DEFINE FIELD process_id ON computer_cli_connection TYPE string;
DEFINE FIELD issued_at ON computer_cli_connection TYPE datetime;
DEFINE FIELD expires_at ON computer_cli_connection TYPE datetime;
DEFINE FIELD closed_at ON computer_cli_connection TYPE option<datetime>;
DEFINE INDEX computer_cli_connection_active ON computer_cli_connection FIELDS grant_id, expires_at;
