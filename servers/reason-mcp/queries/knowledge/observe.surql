LET $at = time::now();
LET $admitted = SELECT *,
    fn::kernel::tasks::lifecycle_v1(task, $reason_server, $tenant, [$reason_task_type], settlement.expected_result) AS lifecycle,
    fn::kernel::artifacts::read_v1(settlement.results, $tenant, $principals, $groups, $clearance, $context, $context_key, $at) AS artifact
FROM reason_analysis WHERE identity.tenant = $tenant AND task_type = $reason_task_type AND settlement.status = 'succeeded' AND settlement.outcome = 'product'
    AND ($analysis = NONE OR task = $analysis) TIMEOUT 10s;
LET $rows = SELECT task, settlement, lifecycle, artifact.observation AS artifact, artifact.deadline AS deadline
FROM $admitted WHERE lifecycle != NONE AND lifecycle.status = 'succeeded'
    AND (lifecycle.retention_expires_at = NONE OR lifecycle.retention_expires_at > $at)
    AND artifact != NONE AND artifact.metadata = settlement.expected_metadata ORDER BY task TIMEOUT 10s;
LET $deadline = array::min($rows.map(|$row| array::min([
    $row.lifecycle.retention_expires_at ?? time::maximum,
    $row.deadline ?? time::maximum
])));
RETURN [[crypto::sha256(type::string($rows)), array::len($rows) > 0,
    IF $deadline = time::maximum { NONE } ELSE { $deadline }]];
