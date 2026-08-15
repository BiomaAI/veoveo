-- Resolve the acquisition that produced an immutable Time authority release
-- without scanning the tenant acquisition catalog.

DEFINE INDEX IF NOT EXISTS time_acquisition_staged_release ON TABLE time_acquisition FIELDS tenant, staged_release_key;
