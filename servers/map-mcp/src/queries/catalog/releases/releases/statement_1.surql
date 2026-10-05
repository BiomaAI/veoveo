SELECT record::id(id) AS pointer_id,
    dataset_key AS dataset_id, release_key AS release_id,
    previous_release_key AS previous_release_id, record_version, activated_at,
    release.source_key AS source_id, release.record_version AS release_version,
    release.version_label AS version_label,
    release.source_digest_sha256 AS source_digest_sha256,
    release.valid_from AS valid_from, release.valid_until AS valid_until,
    release.canonical_json AS canonical_json
FROM (
    SELECT *, type::record('map_dataset_release', release_key) AS release
    FROM map_active_release
    WHERE tenant = $tenant AND ($dataset = NONE OR dataset_key = $dataset)
) WHERE release.tenant = $tenant AND release.release_key = release_key
    AND release.dataset_key = dataset_key AND release.state = 'active'
    AND ($source = NONE OR release.source_key = $source)
ORDER BY dataset_id ASC LIMIT $limit TIMEOUT 5s;
