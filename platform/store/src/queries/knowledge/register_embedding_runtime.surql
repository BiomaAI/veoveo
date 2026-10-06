-- Included inside the existing coordinator-fenced owner transaction.
FOR $profile IN $embedding_profiles {
    LET $old = SELECT * FROM ONLY $profile.id;
    IF $old = NONE {
        CREATE ONLY $profile.id CONTENT {document: $profile.document};
    } ELSE IF $old.document != $profile.document {
        THROW 'knowledge_embedding_profile_immutable';
    };
};
FOR $qualification IN $embedding_qualifications {
    LET $old = SELECT * FROM ONLY $qualification.id;
    IF $old = NONE {
        CREATE ONLY $qualification.id CONTENT {
            query_profile: $qualification.query_profile,
            producer_profile: $qualification.producer_profile,
            document: $qualification.document
        };
    } ELSE IF $old.document != $qualification.document
        OR $old.query_profile != $qualification.query_profile
        OR $old.producer_profile != $qualification.producer_profile {
        THROW 'knowledge_embedding_qualification_immutable';
    };
};
