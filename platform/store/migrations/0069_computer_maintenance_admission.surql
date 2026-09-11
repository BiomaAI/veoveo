-- A retained replacement has one immutable target and an exclusive Computer fence.
DEFINE TABLE computer_maintenance SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD operation_id ON computer_maintenance TYPE uuid;
DEFINE FIELD request_id ON computer_maintenance TYPE uuid;
DEFINE FIELD computer_id ON computer_maintenance TYPE uuid;
DEFINE FIELD task ON computer_maintenance TYPE record<task>;
DEFINE FIELD owner_key ON computer_maintenance TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD actor_context ON computer_maintenance TYPE object FLEXIBLE;
DEFINE FIELD execution_authority ON computer_maintenance TYPE object FLEXIBLE;
DEFINE FIELD provider_instance_id ON computer_maintenance TYPE uuid;
DEFINE FIELD source_instance_id ON computer_maintenance TYPE uuid;
DEFINE FIELD source_template_id ON computer_maintenance TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 64;
DEFINE FIELD source_template_fingerprint ON computer_maintenance TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD source ON computer_maintenance TYPE object FLEXIBLE;
DEFINE FIELD target_instance_id ON computer_maintenance TYPE uuid
  ASSERT $value != u'00000000-0000-0000-0000-000000000000' AND $value != $this.computer_id AND $value != $this.source_instance_id;
DEFINE FIELD target_template_id ON computer_maintenance TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 64;
DEFINE FIELD target_template_fingerprint ON computer_maintenance TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD stage ON computer_maintenance TYPE "queued" | "stopping" | "capturing" | "retiring" | "transferring" | "creating" | "restoring" | "adopting" | "succeeded" | "cancelled" | "recovery_required" DEFAULT "queued";
DEFINE FIELD created_at ON computer_maintenance TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON computer_maintenance TYPE datetime DEFAULT time::now();
DEFINE INDEX computer_maintenance_identity ON computer_maintenance FIELDS operation_id UNIQUE;
DEFINE INDEX computer_maintenance_task ON computer_maintenance FIELDS task UNIQUE;
DEFINE INDEX computer_maintenance_pending ON computer_maintenance FIELDS provider_instance_id, stage, operation_id;

DEFINE TABLE computer_maintenance_request SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD operation_id ON computer_maintenance_request TYPE uuid;
DEFINE FIELD fingerprint ON computer_maintenance_request TYPE string ASSERT string::len($value) = 64;
