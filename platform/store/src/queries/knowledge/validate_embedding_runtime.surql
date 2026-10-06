-- Every retained producer must be covered before data publication or ranking.
IF $gen.space_revision != $embedding_space_revision
    OR $gen.producer_profiles.any(|$profile| $profile NOT IN $embedding_qualified_producers) {
    THROW 'knowledge_embedding_producer_unqualified';
};
FOR $profile IN $embedding_profiles {
    LET $stored = SELECT * FROM ONLY $profile.id;
    IF $stored = NONE OR $stored.document != $profile.document {
        THROW 'knowledge_embedding_profile_missing_or_changed';
    };
};
FOR $qualification IN $embedding_qualifications {
    LET $stored = SELECT * FROM ONLY $qualification.id;
    IF $stored = NONE OR $stored.document != $qualification.document
        OR $stored.query_profile != $qualification.query_profile
        OR $stored.producer_profile != $qualification.producer_profile {
        THROW 'knowledge_embedding_qualification_missing_or_changed';
    };
};
