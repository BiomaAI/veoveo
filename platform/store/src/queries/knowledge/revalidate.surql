BEGIN TRANSACTION;
LET $leader = (SELECT * FROM ONLY $coordinator WHERE owner = $owner AND epoch = $owner_epoch AND expires_at > time::now());
IF $leader = NONE { THROW 'knowledge_coordinator_fenced'; };
-- Write the row to serialize this transaction against takeover and expiry renewal.
UPDATE ONLY $coordinator SET expires_at = $leader.expires_at;

IF $collection.approved != true OR $collection.revision != $approval
    OR $sync.owner_epoch != $owner_epoch OR $sync.epoch != $source_epoch
    OR $member.epoch != $epoch OR $member.stale != true
    OR $member.observation != $previous OR $member.generation.state NOT IN ['building', 'active'] {
    THROW 'knowledge_revalidation_fenced';
};
UPDATE type::table($chunk_table) SET observation = $observation WHERE member = $member;
UPDATE ONLY $member SET stale = false, deleted = false, observation = $observation, valid_until = $valid_until;
COMMIT TRANSACTION;
