-- Durable ownership is independent of the caller's current permission bundle.
-- The original invocation digest remains immutable admission/retry evidence.
DEFINE FIELD ownership ON gateway_task_route TYPE option<object>;
DEFINE FIELD ownership.version ON gateway_task_route TYPE int ASSERT $value = 1;
DEFINE FIELD ownership.principal_key ON gateway_task_route TYPE string;
DEFINE FIELD ownership.principal_kind ON gateway_task_route TYPE "user" | "service";
DEFINE FIELD ownership.issuer ON gateway_task_route TYPE string;
DEFINE FIELD ownership.subject ON gateway_task_route TYPE string;
DEFINE FIELD ownership.invocation_mode ON gateway_task_route TYPE "direct" | "delegated" | "automated";
DEFINE FIELD ownership.initiator ON gateway_task_route TYPE option<string>;
DEFINE FIELD ownership.delegation_id ON gateway_task_route TYPE option<string>;
DEFINE FIELD ownership.data_labels ON gateway_task_route TYPE array<string>;
