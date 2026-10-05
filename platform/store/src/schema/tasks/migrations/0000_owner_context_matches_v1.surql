-- Whole snapshot agreement; this fact grants neither mutation nor caller read authority.
DEFINE FUNCTION fn::kernel::tasks::owner_context_matches_v1($task: record<task>, $expected_owner_context: object) -> bool {
    LET $row = SELECT * FROM ONLY $task;
    RETURN IF type::is_object($row) THEN $row.owner_context = $expected_owner_context ELSE false END;
} PERMISSIONS FULL;
