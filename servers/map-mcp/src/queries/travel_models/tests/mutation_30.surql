UPDATE ONLY $task SET result.payload.structuredContent.travel_model_uri = 'map://travel-model/travel-model-0195dabe-7777-7abc-8def-ffffffffffff' RETURN NONE;
UPDATE map_travel_model_task SET settlement.expected_result = $task.result.payload WHERE task = $task RETURN NONE;
