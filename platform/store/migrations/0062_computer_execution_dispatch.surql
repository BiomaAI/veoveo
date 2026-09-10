DEFINE FIELD OVERWRITE stage ON computer_execution TYPE "queued" | "dispatched";
DEFINE FIELD dispatch_id ON computer_execution TYPE option<uuid>;
DEFINE FIELD dispatched_at ON computer_execution TYPE option<datetime>;
DEFINE FIELD execution_deadline ON computer_execution TYPE option<datetime>;
DEFINE FIELD effective_limits ON computer_execution TYPE option<object> FLEXIBLE;
DEFINE FIELD dispatch_authority ON computer_execution TYPE option<object> FLEXIBLE;
