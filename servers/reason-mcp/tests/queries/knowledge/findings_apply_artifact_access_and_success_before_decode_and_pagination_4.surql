UPDATE $artifact SET metadata.provenance.analysisId = 'wrong-parent' RETURN NONE;
 UPDATE $task SET result.payload.structuredContent.model_uri = 17 RETURN NONE;
