SELECT pointer_id, pointer_version, dataset_key,
    release.release_key AS release_key, release.source_key AS source_key,
    release.state AS release_state, release.valid_from AS valid_from,
    release.valid_until AS valid_until, release.record_version AS release_version,
    source.record_version AS source_version, source.map_families AS source_families,
    array::intersect(source.map_families, $families) AS families,
    release.canonical_json AS release_json, source.canonical_json AS source_json
FROM (
    SELECT pointer_id, pointer_version, dataset_key, release, source FROM (
        SELECT pointer_id, pointer_version, dataset_key, release,
            type::record('map_source', release.source_key) AS source FROM (
            SELECT record::id(id) AS pointer_id, record_version AS pointer_version,
                dataset_key, release_key,
                type::record('map_dataset_release', release_key) AS release
            FROM map_active_release WHERE tenant = $tenant
        ) WHERE release.tenant = $tenant AND release.release_key = release_key
            AND release.dataset_key = dataset_key AND release.state = 'active'
            AND release.valid_from <= $at
            AND (release.valid_until = NONE OR release.valid_until > $at)
    ) WHERE source.tenant = $tenant AND source.enabled = true
        AND source.source_key = release.source_key AND source.dataset_key = dataset_key
) WHERE source.map_families CONTAINSANY $families
ORDER BY release_key ASC TIMEOUT 5s;
