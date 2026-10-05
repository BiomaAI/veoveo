RETURN array::len(SELECT id FROM media_task WHERE task IN $tasks AND (association != NONE OR settlement != NONE));
