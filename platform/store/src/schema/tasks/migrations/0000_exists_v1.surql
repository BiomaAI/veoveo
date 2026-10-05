DEFINE FUNCTION fn::kernel::tasks::exists_v1($task: record<task>) -> bool {
    LET $current = SELECT * FROM ONLY $task;
    RETURN type::is_object($current);
} PERMISSIONS FULL;
