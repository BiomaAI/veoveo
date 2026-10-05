
BEGIN TRANSACTION;
LET $lease = (SELECT * FROM ONLY $agent WHERE lease_owner = $owner AND fence = $fence AND lease_expires_at > $now);
IF $lease = NONE { THROW 'agent lease lost'; };
LET $current = SELECT * FROM ONLY $episode;
IF $current.agent = $agent AND $current.state = 'stopped' {
    UPDATE ONLY $agent SET state = 'idle', revision += 1, updated_at = $now WHERE last_episode = $episode AND lease_owner = $owner AND fence = $fence RETURN NONE;
    RETURN true;
};
LET $claimed_wakes = (SELECT VALUE id FROM wake WHERE id IN $wakes AND agent = $agent AND state = 'claimed' AND claimed_by = $owner AND claim_fence = $fence);
IF array::len($claimed_wakes) != array::len($wakes) { THROW 'wake claim lost'; };
fn::agent_consume_results($agent, $claimed_wakes, $episode, $now);
LET $finished = (UPDATE ONLY $episode SET state = $state, final_output = $output, summary = $summary, input_tokens = $input_tokens, output_tokens = $output_tokens, completion_calls = $completion_calls, tool_calls = $tool_calls, error = $error, finished_at = $now, revision += 1 WHERE state = 'running' RETURN AFTER);
IF $finished = NONE { THROW 'episode completion conflict'; };
UPDATE wake SET state = 'acked', acked_at = $now, acked_by_episode = $episode, claimed_by = NONE, claimed_at = NONE, claim_expires_at = NONE, claim_fence = NONE, updated_at = $now, revision += 1 WHERE id IN $claimed_wakes RETURN NONE;
UPDATE ONLY $agent SET state = 'idle', revision += 1, updated_at = $now WHERE lease_owner = $owner AND fence = $fence RETURN NONE;
COMMIT TRANSACTION;
