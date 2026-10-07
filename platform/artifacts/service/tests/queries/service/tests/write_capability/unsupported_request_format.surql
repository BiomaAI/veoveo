DEFINE FIELD OVERWRITE request_format ON artifact_write_redemption TYPE option<string> PERMISSIONS FULL;
UPDATE $redemption SET request_format = 'veoveo.ai/artifact-write-request/v1' RETURN NONE;
