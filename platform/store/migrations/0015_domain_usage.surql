-- Replica-safe usage projections for deterministic domain MCP servers.

DEFINE TABLE IF NOT EXISTS domain_usage SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE domain_usage TYPE uuid DEFAULT rand::uuid();
DEFINE FIELD IF NOT EXISTS tenant ON TABLE domain_usage TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS task ON TABLE domain_usage TYPE record<task>;
DEFINE FIELD IF NOT EXISTS server ON TABLE domain_usage TYPE record<mcp_server>;
DEFINE FIELD IF NOT EXISTS source_id ON TABLE domain_usage TYPE option<string>;
DEFINE FIELD IF NOT EXISTS provider_job_id ON TABLE domain_usage TYPE option<string>;
DEFINE FIELD IF NOT EXISTS model_id ON TABLE domain_usage TYPE string;
DEFINE FIELD IF NOT EXISTS kind ON TABLE domain_usage TYPE "estimate" | "actual";
DEFINE FIELD IF NOT EXISTS quantity ON TABLE domain_usage TYPE option<float>;
DEFINE FIELD IF NOT EXISTS unit ON TABLE domain_usage TYPE option<string>;
DEFINE FIELD IF NOT EXISTS amount ON TABLE domain_usage TYPE option<float>;
DEFINE FIELD IF NOT EXISTS currency ON TABLE domain_usage TYPE option<string>;
DEFINE FIELD IF NOT EXISTS metadata ON TABLE domain_usage TYPE object FLEXIBLE DEFAULT {};
DEFINE FIELD IF NOT EXISTS recorded_at ON TABLE domain_usage TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE domain_usage TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS domain_usage_task_time ON TABLE domain_usage FIELDS task, recorded_at;
DEFINE INDEX IF NOT EXISTS domain_usage_server_task ON TABLE domain_usage FIELDS server, task;
DEFINE INDEX IF NOT EXISTS domain_usage_tenant_time ON TABLE domain_usage FIELDS tenant, recorded_at;
