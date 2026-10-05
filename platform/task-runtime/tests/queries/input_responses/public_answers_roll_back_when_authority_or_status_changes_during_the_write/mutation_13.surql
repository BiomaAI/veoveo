DEFINE EVENT OVERWRITE input_interleave ON task_input
                WHEN $event = 'UPDATE' AND $before.response = NONE AND $after.response != NONE
                THEN (UPDATE ONLY $after.task SET task_type = 'other-operation' RETURN NONE);
