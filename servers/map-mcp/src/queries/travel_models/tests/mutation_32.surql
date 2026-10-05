UPDATE ONLY $task SET request.input.request.travel_model_id = 'malformed', result.payload.structuredContent.travel_model_id = 'malformed' RETURN NONE;
UPDATE map_travel_model_task SET settlement.expected_result = $task.result.payload WHERE task = $task RETURN NONE;
UPDATE map_travel_model_task SET identity.expected_input = $task.request.input, identity.travel_model_id = 'malformed' WHERE task = $task RETURN NONE;
