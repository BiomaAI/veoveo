-- Retention selects bounded record IDs by kind and age before deleting them.
DEFINE INDEX IF NOT EXISTS audit_event_resource_time ON TABLE audit_event FIELDS resource_type, occurred_at;
