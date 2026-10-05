LET $at = time::now();
RETURN (SELECT * FROM artifact_occurrence
WHERE tenant = $tenant AND fn::kernel::artifacts::read_v1(id, $tenant, $principals, $groups, $clearance, $context, $context_key, $at) != NONE
    AND ($artifact = NONE OR id = $artifact)
    AND ($cursor = NONE OR id < $cursor)
ORDER BY id DESC
LIMIT $limit).map(|$occurrence| {
    occurrence: $occurrence,
    blob: (SELECT * FROM ONLY $occurrence.blob),
    tenant: (SELECT * FROM ONLY $occurrence.tenant),
    grants: (SELECT * FROM artifact_grant WHERE in = $occurrence.id ORDER BY id)
});
