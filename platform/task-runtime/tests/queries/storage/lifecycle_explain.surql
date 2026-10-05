-- The trusted leaf's direct record read, with the same identity predicates.
SELECT * FROM $task WHERE id = $task AND server = $server
    AND tenant = $tenant AND task_type IN $types EXPLAIN;
