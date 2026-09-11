-- Run with command workers drained. The migration runner supplies one transaction.
LET $completed_commands = (SELECT id, execution_id, task, result FROM computer_execution WHERE stage = "completed" AND result != NONE) ?? [];
IF array::len($completed_commands) > 0 {
FOR $command IN $completed_commands {
    LET $execution_record = $command.id;
    LET $command_task = $command.task;
    LET $uri = string::concat("computer://executions/", <string>$command.execution_id);
    LET $saved_task = SELECT * FROM ONLY $command_task;
    IF $saved_task != NONE AND $saved_task.status = "succeeded" {
        IF $saved_task.server != mcp_server:computers OR $saved_task.task_type != "computer.execution"
            OR $saved_task.result.structuredContent != $command.result
            OR ($saved_task.result.isError ?? false) != ($command.result.exitCode != 0) {
            THROW "computer_execution_result_migration_conflict";
        };
        UPDATE ONLY $command_task SET result.structuredContent.result_uri = $uri,
            result.content = [
                { type: "text", text: string::concat("Command exited with code ", <string>$command.result.exitCode) },
                { type: "resource_link", uri: $uri, name: "Command result" }
            ];
    };
    UPDATE ONLY $execution_record SET result.result_uri = $uri;
};
};
