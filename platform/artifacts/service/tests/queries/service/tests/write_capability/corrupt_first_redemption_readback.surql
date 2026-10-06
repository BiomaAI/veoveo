DEFINE EVENT OVERWRITE corrupt_write_readback ON artifact_write_redemption
WHEN $event = 'CREATE'
THEN { UPDATE $after.id SET task_id = 'not-a-task'; };
