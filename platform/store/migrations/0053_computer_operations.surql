-- Durable operation intent is separate from a Task's worker lease.
DEFINE TABLE computer_operation SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD operation_id ON computer_operation TYPE uuid;
DEFINE FIELD computer_id ON computer_operation TYPE uuid;
DEFINE FIELD task ON computer_operation TYPE record<task>;
DEFINE FIELD actor_context ON computer_operation TYPE object FLEXIBLE;
DEFINE FIELD provider_instance_id ON computer_operation TYPE uuid;
DEFINE FIELD template_fingerprint ON computer_operation TYPE string
  ASSERT string::len($value) = 64;
DEFINE FIELD action ON computer_operation TYPE "create" | "start" | "stop";
DEFINE FIELD stage ON computer_operation TYPE "queued" | "dispatched" | "recovery_required" | "succeeded" | "failed" | "cancelled" DEFAULT "queued";
DEFINE FIELD previous_phase ON computer_operation TYPE "reserved" | "stopped" | "ready";
DEFINE FIELD previous_resource_id ON computer_operation TYPE option<string>;
DEFINE FIELD previous_process_id ON computer_operation TYPE option<string>;
DEFINE FIELD created_at ON computer_operation TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON computer_operation TYPE datetime DEFAULT time::now();
DEFINE INDEX computer_operation_identity ON computer_operation FIELDS operation_id UNIQUE;
DEFINE INDEX computer_operation_task ON computer_operation FIELDS task UNIQUE;
DEFINE INDEX computer_operation_stage ON computer_operation FIELDS stage, operation_id;

DEFINE TABLE computer_operation_request SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD operation_id ON computer_operation_request TYPE uuid;
DEFINE FIELD fingerprint ON computer_operation_request TYPE string
  ASSERT string::len($value) = 64;
