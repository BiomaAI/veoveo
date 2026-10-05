DEFINE FUNCTION fn::kernel::tasks::input_matches_v1($task: record<task>, $expected_input: object) -> bool {
    LET $current = SELECT * FROM ONLY $task;
    RETURN IF type::is_object($current) THEN {
        LET $request = $current.request;
        RETURN IF type::is_object($request) THEN $request.input = $expected_input ELSE false END;
    } ELSE false END;
} PERMISSIONS FULL;
