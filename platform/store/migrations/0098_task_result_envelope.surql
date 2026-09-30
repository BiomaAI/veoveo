-- This format cut requires a fresh Task store. Ambiguous results are not converted.
IF array::len((SELECT VALUE id FROM task LIMIT 1)) > 0
    OR array::len((SELECT VALUE id FROM outbox_event WHERE aggregate_type = "task" LIMIT 1)) > 0 {
    THROW "task_result_reset_required: stop Task writers and rebuild the platform database before installing this release";
};

DEFINE FIELD OVERWRITE result ON TABLE task TYPE option<object> FLEXIBLE
    ASSERT $value = NONE OR object::keys($value) = ["payload"];
DEFINE INDEX OVERWRITE optimization_task_solution ON TABLE task
    FIELDS server, result.payload.structuredContent.result_uri;
