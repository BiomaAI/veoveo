DEFINE EVENT grant_failure ON recording_read_grant WHEN $event = 'CREATE' THEN { THROW 'injected_grant_failure'; };
