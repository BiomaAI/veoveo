-- Console audit filtering uses bounded typed records from the snapshot. No
-- canonical query consumes this full-text index, while every policy and
-- authentication decision paid its write cost.

REMOVE INDEX IF EXISTS audit_event_search ON TABLE audit_event;
