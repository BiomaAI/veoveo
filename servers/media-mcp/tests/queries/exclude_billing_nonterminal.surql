DELETE media_usage WHERE task = $task;
-- Local deletion acknowledgement does not establish a provider terminal outcome.
UPDATE ONLY $job SET state = 'cancelled', cancellation_receipt = {deleted_count: 1}, terminal_event = NONE, observed_at = NONE, provider_payload = {} RETURN NONE;
