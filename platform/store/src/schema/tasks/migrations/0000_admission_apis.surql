DEFINE FUNCTION fn::kernel::tasks::release_retention_v1($task: record<task>, $expected_server: record<mcp_server>, $pin: string) -> bool {
    LET $source_task = SELECT * FROM ONLY $task;
    RETURN IF type::is_object($source_task) THEN {
        IF $source_task.server != $expected_server { THROW 'canonical task server changed during retention release'; };
        IF !($source_task.retention_pins CONTAINS $pin) { THROW 'canonical task retention pin missing during release'; };
        LET $released = UPDATE ONLY $task SET retention_pins -= $pin WHERE server = $expected_server AND retention_pins CONTAINS $pin RETURN AFTER;
        IF $released = NONE { THROW 'canonical task retention release failed'; };
        RETURN true;
    } ELSE { THROW 'canonical task missing during retention release'; } END;
} PERMISSIONS FULL;
