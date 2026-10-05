UPDATE $artifact SET retention_expires_at = time::now() - 1s RETURN NONE;
 UPDATE $task SET result.payload.structuredContent.model_uri = 17 RETURN NONE;
