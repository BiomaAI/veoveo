UPDATE ONLY $operation SET progress.steps[0].observation_reads = 7, progress.steps[0].last_observation_id = $id, progress.steps[0].next_observation_at = <string>(time::now() - 1s);
