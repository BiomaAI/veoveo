-- Core Computers: retained collection identity and transactional admission.
DEFINE TABLE computer SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
  PERMISSIONS NONE;
DEFINE FIELD computer_id ON computer TYPE uuid;
DEFINE FIELD owner_key ON computer TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD tenant_key ON computer TYPE string;
DEFINE FIELD owner_context ON computer TYPE object FLEXIBLE;
DEFINE FIELD provider_instance_id ON computer TYPE uuid;
DEFINE FIELD template_id ON computer TYPE string
  ASSERT string::len($value) > 0
  AND string::len($value) <= 64;
DEFINE FIELD template_fingerprint ON computer TYPE string
  ASSERT string::len($value) = 64;
DEFINE FIELD phase ON computer TYPE "reserved" | "provisioning" | "starting" | "ready" | "stopping" | "stopped" | "error" | "recovery_required" DEFAULT "reserved";
DEFINE FIELD provider_resource_id ON computer TYPE option<string>;
DEFINE FIELD process_id ON computer TYPE option<string>;
DEFINE FIELD active_operation ON computer TYPE option<uuid>;
DEFINE FIELD created_at ON computer TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON computer TYPE datetime DEFAULT time::now();
DEFINE INDEX computer_identity ON computer FIELDS computer_id UNIQUE;
DEFINE INDEX computer_owner_page ON computer FIELDS owner_key, computer_id;
DEFINE INDEX computer_active_operation ON computer FIELDS active_operation;

DEFINE TABLE computer_request SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD computer ON computer_request TYPE record<computer>;
DEFINE FIELD fingerprint ON computer_request TYPE string
  ASSERT string::len($value) = 64;

DEFINE TABLE computer_usage SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD retained ON computer_usage TYPE int ASSERT $value >= 0;

DEFINE TABLE computer_capacity SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD fingerprint ON computer_capacity TYPE string
  ASSERT string::len($value) = 64;
DEFINE FIELD per_owner ON computer_capacity TYPE int ASSERT $value >= 0;
DEFINE FIELD per_tenant ON computer_capacity TYPE int ASSERT $value >= 0;
DEFINE FIELD provider ON computer_capacity TYPE int ASSERT $value >= 0;
DEFINE FIELD admissions ON computer_capacity TYPE int ASSERT $value >= 0;
