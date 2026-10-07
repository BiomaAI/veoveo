DEFINE FIELD OVERWRITE request_format ON artifact_write_redemption TYPE option<string> PERMISSIONS FULL;
UPDATE $redemption SET request_format = NONE RETURN NONE;
