UPDATE ONLY $task SET result = $result RETURN NONE;
UPDATE media_task SET settlement = {status: 'succeeded', completed_at: time::now(), prediction: association.prediction, expected_result: $expected}
WHERE task = $task RETURN NONE;
