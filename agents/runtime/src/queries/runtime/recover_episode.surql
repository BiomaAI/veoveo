BEGIN TRANSACTION; UPDATE ONLY $episode SET state = 'crashed', error = 'scheduler lease was recovered', finished_at = $now, revision += 1 WHERE state = 'running' RETURN NONE; COMMIT TRANSACTION;
