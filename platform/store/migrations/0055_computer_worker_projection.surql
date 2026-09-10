-- A known pre-dispatch refusal is distinct from an uncertain provider effect.
DEFINE FIELD completion_code ON computer_operation TYPE option<"authority_denied" | "cancelled_before_dispatch">;
DEFINE FIELD task_projected_at ON computer_operation TYPE option<datetime>;
DEFINE INDEX computer_operation_provider ON computer_operation FIELDS provider_instance_id, operation_id;
