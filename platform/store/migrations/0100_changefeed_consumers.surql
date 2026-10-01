-- Consumer acknowledgements must not generate changes or wake themselves.
DEFINE TABLE changefeed_checkpoint SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD versionstamp ON changefeed_checkpoint TYPE int ASSERT $value >= 0;
DEFINE FIELD updated_at ON changefeed_checkpoint TYPE datetime;

-- Native recovery for lifecycle and authority sources.
ALTER TABLE agent_definition CHANGEFEED 30d;
ALTER TABLE managed_agent CHANGEFEED 30d;
ALTER TABLE managed_agent_operation CHANGEFEED 30d;
ALTER TABLE work_context CHANGEFEED 30d;
ALTER TABLE computer CHANGEFEED 30d INCLUDE ORIGINAL;
ALTER TABLE computer_automation_grant CHANGEFEED 30d INCLUDE ORIGINAL;
ALTER TABLE computer_session_grant CHANGEFEED 30d INCLUDE ORIGINAL;
ALTER TABLE computer_cli_grant CHANGEFEED 30d INCLUDE ORIGINAL;
ALTER TABLE computer_maintenance CHANGEFEED 30d INCLUDE ORIGINAL;

-- Execution authority watches need identity on deletion, without original payloads.
ALTER TABLE computer_execution CHANGEFEED 30d;
ALTER TABLE computer_file_transfer CHANGEFEED 30d;
ALTER TABLE computer_automation_policy CHANGEFEED 30d;
ALTER TABLE computer_session_grant_policy CHANGEFEED 30d;
