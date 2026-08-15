-- Bounded domain discovery over durable tasks. These indexes retain the
-- Optimization vocabulary stored in the canonical task request and result;
-- they do not introduce a parallel resource identity table.

DEFINE INDEX IF NOT EXISTS task_domain_owner_page ON TABLE task FIELDS server, tenant, owner, profile, work_context, created_at, id;
DEFINE INDEX IF NOT EXISTS optimization_task_problem ON TABLE task FIELDS server, request.input.common.problem_id;
DEFINE INDEX IF NOT EXISTS optimization_task_run ON TABLE task FIELDS server, request.input.common.run_id;
DEFINE INDEX IF NOT EXISTS optimization_task_solution ON TABLE task FIELDS server, result.structuredContent.result_uri;
