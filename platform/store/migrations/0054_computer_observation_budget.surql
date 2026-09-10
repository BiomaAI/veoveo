-- A process restart never creates a new dispatch or recovery budget.
DEFINE FIELD dispatch_id ON computer_operation TYPE option<uuid>;
DEFINE FIELD dispatched_at ON computer_operation TYPE option<datetime>;
DEFINE FIELD observation_deadline ON computer_operation TYPE option<datetime>;
DEFINE FIELD observation_reads ON computer_operation TYPE int DEFAULT 0 ASSERT $value >= 0 AND $value <= 8;
DEFINE FIELD next_observation_at ON computer_operation TYPE option<datetime>;
DEFINE FIELD last_observation_id ON computer_operation TYPE option<uuid>;
DEFINE FIELD settled_at ON computer_operation TYPE option<datetime>;
DEFINE FIELD result_resource_id ON computer_operation TYPE option<string>;
DEFINE FIELD result_process_id ON computer_operation TYPE option<string>;
UPDATE computer_operation SET observation_reads = 0 WHERE observation_reads = NONE;
