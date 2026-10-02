LET $at = time::now();
LET $rows = SELECT retention_expires_at,
    (SELECT VALUE expires_at FROM artifact_grant
        WHERE in = $parent.id AND expires_at > $at) AS deadlines
FROM artifact_occurrence
WHERE {{ADMISSION}} AND ($all OR id IN $members)
TIMEOUT 10s;
LET $deadline = array::min($rows.map(|$row| array::min([
    $row.retention_expires_at ?? time::maximum,
    array::min($row.deadlines) ?? time::maximum
])));
RETURN [IF $deadline = time::maximum { NONE } ELSE { $deadline }];
