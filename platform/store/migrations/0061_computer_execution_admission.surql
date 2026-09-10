DEFINE TABLE computer_execution SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD execution_id ON computer_execution TYPE uuid;
DEFINE FIELD computer_id ON computer_execution TYPE uuid;
DEFINE FIELD provider_instance_id ON computer_execution TYPE uuid;
DEFINE FIELD actor_key ON computer_execution TYPE string ASSERT string::len($value) = 64;
DEFINE FIELD binding ON computer_execution TYPE object FLEXIBLE;
DEFINE FIELD authority ON computer_execution TYPE object FLEXIBLE;
DEFINE FIELD sealed ON computer_execution TYPE object FLEXIBLE;
DEFINE FIELD task ON computer_execution TYPE record<task>;
DEFINE FIELD stage ON computer_execution TYPE "queued";
DEFINE FIELD created_at ON computer_execution TYPE datetime;
DEFINE FIELD updated_at ON computer_execution TYPE datetime;
DEFINE INDEX execution_identity ON computer_execution FIELDS execution_id UNIQUE;
DEFINE INDEX execution_pending ON computer_execution FIELDS provider_instance_id, execution_id;
DEFINE INDEX execution_actor ON computer_execution FIELDS actor_key, computer_id, execution_id;

DEFINE TABLE computer_execution_request SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD execution ON computer_execution_request TYPE record<computer_execution>;

DEFINE TABLE computer_execution_slot SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD execution ON computer_execution_slot TYPE record<computer_execution>;
DEFINE FIELD computer_id ON computer_execution_slot TYPE uuid;
DEFINE INDEX execution_computer_slot ON computer_execution_slot FIELDS computer_id UNIQUE;
