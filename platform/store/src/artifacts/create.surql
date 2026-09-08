BEGIN TRANSACTION;
  INSERT IGNORE INTO artifact_blob $blob_content RETURN NONE;
  LET $retained = SELECT * FROM ONLY $blob;
  IF $retained = NONE
  OR $retained.tenant != $blob_content.tenant
  OR $retained.sha256 != $blob_content.sha256
  OR $retained.byte_len != $blob_content.byte_len {
    THROW 'artifact_blob_integrity_conflict';
  };
  CREATE ONLY $artifact CONTENT $artifact_content RETURN NONE;
  INSERT RELATION INTO artifact_grant $grants RETURN NONE;
  CREATE outbox_event CONTENT $outbox RETURN NONE;
COMMIT TRANSACTION;
