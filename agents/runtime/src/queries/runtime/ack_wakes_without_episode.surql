
BEGIN TRANSACTION;
LET $lease = (SELECT * FROM ONLY $agent WHERE lease_owner = $owner AND fence = $fence AND lease_expires_at > $now);
IF $lease = NONE { THROW 'agent lease lost'; };
LET $claimed_wakes = (SELECT VALUE id FROM wake WHERE id IN $wakes AND agent = $agent AND state = 'claimed' AND claimed_by = $owner AND claim_fence = $fence);
IF array::len($claimed_wakes) != array::len($wakes) { THROW 'wake claim lost'; };
UPDATE wake SET state = 'acked', acked_at = $now, acked_by_episode = NONE, claimed_by = NONE, claimed_at = NONE, claim_expires_at = NONE, claim_fence = NONE, updated_at = $now, revision += 1 WHERE id IN $claimed_wakes RETURN NONE;
UPDATE ONLY $agent SET state = 'idle', revision += 1, updated_at = $now WHERE lease_owner = $owner AND fence = $fence RETURN NONE;
COMMIT TRANSACTION;
