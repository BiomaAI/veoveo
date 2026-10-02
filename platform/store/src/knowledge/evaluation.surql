BEGIN TRANSACTION;
  LET $source = (
    SELECT *
    FROM ONLY $generation
    WHERE
      tenant = $tenant
      AND revision = $specification
  );
  IF $source = NONE {
    THROW 'knowledge_evaluation_generation_mismatch';
  };
  LET $previous = (
    SELECT *
    FROM ONLY $evaluation
  );
  IF $previous = NONE {
    CREATE ONLY $evaluation CONTENT {
      tenant: $tenant,
      generation: $generation,
      document: $document,
    };
  } ELSE IF $previous.tenant != $tenant
  OR $previous.generation != $generation
  OR $previous.document != $document {
    THROW 'knowledge_evaluation_identity_conflict';
  };
COMMIT TRANSACTION;
