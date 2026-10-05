UPDATE ONLY $layer SET state = 'committed', artifact = $artifact, byte_len = 1024, message_count = 3, sha256 = $digest, schema_digest = $digest, rrd_version = '0.38.1';
