-- Operational search and dashboard queue counts. These are isolated so their
-- write-amplification cost can be measured independently from base schema work.

DEFINE ANALYZER IF NOT EXISTS platform_search TOKENIZERS class, blank FILTERS lowercase, ascii;
DEFINE INDEX IF NOT EXISTS artifact_occurrence_search ON TABLE artifact_occurrence FIELDS search_text FULLTEXT ANALYZER platform_search BM25;
DEFINE INDEX IF NOT EXISTS task_search ON TABLE task FIELDS search_text FULLTEXT ANALYZER platform_search BM25;
DEFINE INDEX IF NOT EXISTS audit_event_search ON TABLE audit_event FIELDS search_text FULLTEXT ANALYZER platform_search BM25;

DEFINE INDEX IF NOT EXISTS task_count ON TABLE task COUNT;
DEFINE INDEX IF NOT EXISTS task_queued_count ON TABLE task COUNT WHERE status = "queued";
DEFINE INDEX IF NOT EXISTS task_running_count ON TABLE task COUNT WHERE status = "running";
DEFINE INDEX IF NOT EXISTS task_waiting_count ON TABLE task COUNT WHERE status = "waiting";
DEFINE INDEX IF NOT EXISTS wake_pending_count ON TABLE wake COUNT WHERE state = "pending";
DEFINE INDEX IF NOT EXISTS artifact_occurrence_count ON TABLE artifact_occurrence COUNT;
DEFINE INDEX IF NOT EXISTS share_link_count ON TABLE share_link COUNT;
