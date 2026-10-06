DEFINE EVENT OVERWRITE corrupt_write_readback ON artifact_write_redemption
WHEN $event = 'UPDATE' AND $before.request_hash != $after.request_hash
THEN { UPDATE $after.capability SET task_id = '01900000000070008000000000000001'; };
