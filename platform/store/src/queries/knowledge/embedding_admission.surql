BEGIN TRANSACTION;
-- REGISTER_EMBEDDING_RUNTIME
LET $gen = SELECT * FROM ONLY $generation WHERE tenant = $tenant AND state IN ['building','active'];
IF $gen = NONE { THROW 'knowledge_embedding_generation_missing'; };
LET $profiles = SELECT * FROM $gen.producer_profiles;
LET $qualifications = SELECT * FROM knowledge_embedding_qualification
    WHERE id IN $embedding_qualification_records AND producer_profile IN $gen.producer_profiles;
RETURN { epoch: $gen.profile_epoch, space_revision: $gen.space_revision, revision: $gen.revision, document: $gen.document,
    producer_records: $gen.producer_profiles, profiles: $profiles, qualifications: $qualifications };
COMMIT TRANSACTION;
