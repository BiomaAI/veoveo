UPDATE $artifact SET metadata.provenance.analysis_id = 'wrong-parent' RETURN NONE;
 UPDATE $task SET result.payload.structuredContent.model_uri = 17 RETURN NONE;
