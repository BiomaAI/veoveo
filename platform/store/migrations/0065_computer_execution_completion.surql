DEFINE FIELD OVERWRITE stage ON computer_execution TYPE "queued" | "dispatched" | "containing" | "recovery_required" | "failed" | "cancelled" | "completed";
DEFINE FIELD result ON computer_execution TYPE option<object> FLEXIBLE;
