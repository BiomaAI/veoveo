DEFINE FUNCTION fn::kernel::gateway::task_retention_route_v1($route: record<gateway_task_route>) -> object {
    LET $current = SELECT * FROM ONLY $route;
    RETURN IF type::is_object($current) THEN { source_task: $current.source_task, server: $current.server }
    ELSE { THROW 'gateway task route missing during retention release'; } END;
} PERMISSIONS FULL;
