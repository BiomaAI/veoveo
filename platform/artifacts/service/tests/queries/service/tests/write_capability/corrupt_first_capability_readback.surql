DEFINE EVENT OVERWRITE corrupt_write_readback ON artifact_write_redemption
WHEN $event = 'CREATE'
THEN { UPDATE $after.capability SET task_id = '01900000000070008000000000000001'; };
