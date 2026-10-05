BEGIN TRANSACTION;
IF array::len(SELECT VALUE id FROM $ids WHERE partition = $partition) != array::len($ids) {
    THROW 'audit_sealed_record_missing';
};
SELECT id, partition, draft, target_ref, recorded_at FROM $ids
WHERE partition = $partition
    AND ($after = NONE OR id > $after)
    AND ($class = NONE OR class = $class)
    AND ($actor = NONE OR actor_key = $actor)
    AND ($target = NONE OR draft.target = $target)
    AND ($outcome = NONE OR outcome = $outcome)
    AND ($trace = NONE OR trace_id = $trace)
    AND ($from = NONE OR occurred_at >= $from)
    AND ($until = NONE OR occurred_at < $until)
LIMIT 4096;
COMMIT TRANSACTION;
