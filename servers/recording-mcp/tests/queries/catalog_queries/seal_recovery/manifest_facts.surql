BEGIN TRANSACTION;
UPDATE ONLY $blob SET byte_len = $bytes, sha256 = $digest RETURN NONE;
UPDATE ONLY $occurrence SET metadata = $metadata RETURN NONE;
COMMIT TRANSACTION;
