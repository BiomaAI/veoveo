-- Drain old Computers readers before applying: closed step records gain a field.
-- Existing windows keep their original deadline, charges and dispatch identity.
UPDATE computer_maintenance SET progress.steps = array::map(progress.steps,
  |$step| object::extend($step, {observation_started_at: $step.dispatched_at}));

-- Immutable request receipts archive the previous finite window before renewal.
DEFINE TABLE computer_maintenance_resume SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD operation_id ON computer_maintenance_resume TYPE uuid;
DEFINE FIELD request_id ON computer_maintenance_resume TYPE uuid;
DEFINE FIELD fingerprint ON computer_maintenance_resume TYPE string;
DEFINE FIELD input ON computer_maintenance_resume TYPE object FLEXIBLE;
DEFINE FIELD authority ON computer_maintenance_resume TYPE object FLEXIBLE;
DEFINE FIELD decision ON computer_maintenance_resume TYPE object FLEXIBLE;
DEFINE FIELD previous_progress ON computer_maintenance_resume TYPE object FLEXIBLE;
DEFINE FIELD created_at ON computer_maintenance_resume TYPE datetime DEFAULT time::now();
DEFINE INDEX computer_maintenance_resume_operation ON computer_maintenance_resume FIELDS operation_id, created_at;
