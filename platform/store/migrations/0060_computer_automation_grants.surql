-- Named, non-bearer authority for retained Computer automation. Independent of
-- browser/CLI connection grants. This migration alone exposes no new execution.
DEFINE TABLE computer_automation_grant SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD grant_id ON computer_automation_grant TYPE uuid;
DEFINE FIELD computer_id ON computer_automation_grant TYPE uuid;
DEFINE FIELD owner_key ON computer_automation_grant TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD provider_instance_id ON computer_automation_grant TYPE uuid;
DEFINE FIELD authority ON computer_automation_grant TYPE object FLEXIBLE;
DEFINE FIELD grantee ON computer_automation_grant TYPE record<principal>;
DEFINE FIELD principal_id ON computer_automation_grant TYPE string;
DEFINE FIELD oauth_client_id ON computer_automation_grant TYPE string;
DEFINE FIELD grantee_issuer ON computer_automation_grant TYPE string;
DEFINE FIELD grantee_subject ON computer_automation_grant TYPE string;
DEFINE FIELD grantee_kind ON computer_automation_grant TYPE "user" | "service";
DEFINE FIELD name ON computer_automation_grant TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 64;
DEFINE FIELD permissions ON computer_automation_grant TYPE array<"read" | "execute" | "start" | "stop"> ASSERT array::len($value) > 0 AND array::len($value) <= 4;
DEFINE FIELD execution_limits ON computer_automation_grant TYPE option<object> FLEXIBLE;
DEFINE FIELD issued_at ON computer_automation_grant TYPE datetime;
DEFINE FIELD expires_at ON computer_automation_grant TYPE datetime;
DEFINE FIELD revoked_at ON computer_automation_grant TYPE option<datetime>;
DEFINE FIELD revision ON computer_automation_grant TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE INDEX computer_automation_grant_id ON computer_automation_grant FIELDS grant_id UNIQUE;
DEFINE INDEX computer_automation_grant_owner ON computer_automation_grant FIELDS owner_key, computer_id, grant_id;
DEFINE INDEX computer_automation_grant_grantee ON computer_automation_grant FIELDS grantee, computer_id, grant_id;

DEFINE TABLE computer_automation_request SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD grant ON computer_automation_request TYPE record<computer_automation_grant>;
DEFINE FIELD fingerprint ON computer_automation_request TYPE string ASSERT string::len($value) = 64;

DEFINE TABLE computer_automation_guard SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD revision ON computer_automation_guard TYPE int ASSERT $value >= 0;

DEFINE TABLE computer_automation_policy SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD fingerprint ON computer_automation_policy TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD max_grants ON computer_automation_policy TYPE int ASSERT $value >= 0 AND $value <= 64;
DEFINE FIELD maximum_lifetime_seconds ON computer_automation_policy TYPE int ASSERT $value > 0 AND $value <= 86400;
DEFINE FIELD maximum_execution_seconds ON computer_automation_policy TYPE int ASSERT $value > 0 AND $value <= 7200;
DEFINE FIELD maximum_output_bytes ON computer_automation_policy TYPE int ASSERT $value > 0 AND $value <= 67108864;
DEFINE FIELD admissions ON computer_automation_policy TYPE int DEFAULT 0 ASSERT $value >= 0;
