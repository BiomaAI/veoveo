SELECT server, IF server = $server { $this } ELSE { NONE } AS snapshot
FROM $task LIMIT 1;
