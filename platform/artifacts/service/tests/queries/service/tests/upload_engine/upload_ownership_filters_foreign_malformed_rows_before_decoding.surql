DEFINE FIELD OVERWRITE state ON artifact_upload TYPE string; UPDATE $upload SET state = 'invalid-state' RETURN NONE;
