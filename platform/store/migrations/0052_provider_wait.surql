-- Additive stored profile. Deploy readers before admitting provider_wait Tasks.
-- No existing task changes class or cancellation semantics.
DEFINE FIELD OVERWRITE recovery_class ON TABLE task
  TYPE "resume" | "webhook_wait" | "provider_wait" | "interrupted_indeterminate";
