-- Current-only request receipt admission. No default, backfill or conversion.
DEFINE FIELD request_format ON artifact_write_redemption TYPE 'veoveo.ai/artifact-write-request/v2' PERMISSIONS FULL;
