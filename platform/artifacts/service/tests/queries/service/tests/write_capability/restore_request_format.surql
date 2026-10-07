UPDATE $redemption SET request_format = 'veoveo.ai/artifact-write-request/v2' RETURN NONE;
DEFINE FIELD OVERWRITE request_format ON artifact_write_redemption TYPE 'veoveo.ai/artifact-write-request/v2' PERMISSIONS FULL;
