DEFINE FIELD OVERWRITE manifest_digest ON recording_projection_receipt TYPE any; UPDATE $projection SET manifest_digest = 17 RETURN NONE;
