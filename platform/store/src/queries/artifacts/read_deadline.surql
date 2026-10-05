LET $at = time::now();
LET $deadlines = SELECT VALUE {
    LET $member = fn::kernel::artifacts::read_v1(
        id, $tenant, $principals, $groups, $clearance, $context, $context_key, $at);
    RETURN IF type::is_object($member) THEN $member.deadline ELSE NONE END;
}
FROM artifact_occurrence WHERE tenant = $tenant AND ($all OR id IN $members)
TIMEOUT 10s;
RETURN array::min($deadlines.filter(|$deadline| $deadline != NONE));
