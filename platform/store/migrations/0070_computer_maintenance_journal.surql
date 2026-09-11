-- Bounded typed step history is private; sensitive adapter bytes are encrypted separately.
DEFINE FIELD progress ON computer_maintenance TYPE object FLEXIBLE DEFAULT {steps: [], recovery: NULL};
DEFINE FIELD task_projected_at ON computer_maintenance TYPE option<datetime>;
UPDATE computer_maintenance SET progress = {steps: [], recovery: NULL} WHERE progress = NONE;

DEFINE TABLE computer_maintenance_policy SCHEMAFULL TYPE NORMAL PERMISSIONS NONE;
DEFINE FIELD operation_id ON computer_maintenance_policy TYPE uuid;
DEFINE FIELD envelope ON computer_maintenance_policy TYPE object FLEXIBLE;
DEFINE FIELD created_at ON computer_maintenance_policy TYPE datetime DEFAULT time::now();
DEFINE INDEX computer_maintenance_policy_identity ON computer_maintenance_policy FIELDS operation_id UNIQUE;
