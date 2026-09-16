-- Shared human-authored references; Artifact access remains owned by the Artifact plane.
DEFINE FIELD OVERWRITE text ON workspace_message TYPE string ASSERT string::len($value) <= 32768;
DEFINE FIELD attachments ON workspace_message TYPE option<array<object>>;
DEFINE FIELD attachments.*.artifact ON workspace_message TYPE uuid;
DEFINE FIELD attachments.*.name ON workspace_message TYPE string;
