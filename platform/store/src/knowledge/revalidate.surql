BEGIN TRANSACTION;
__LEASE__
IF $collection.approved != true OR $collection.revision != $approval
    OR $sync.owner_epoch != $owner_epoch OR $sync.epoch != $source_epoch
    OR $member.epoch != $epoch OR $member.stale != true
    OR $member.observation != $previous OR $member.generation.state NOT IN ['building', 'active'] {
    THROW 'knowledge_revalidation_fenced';
};
UPDATE __TABLE__ SET observation = $observation WHERE member = $member;
UPDATE ONLY $member SET stale = false, deleted = false, observation = $observation, valid_until = $valid_until;
COMMIT TRANSACTION;
