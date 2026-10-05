SELECT dataset_kind, $this AS pointer,
    (SELECT * FROM type::record('time_authority_release', $parent.release_key)
        WHERE tenant = $tenant AND dataset_kind = $parent.dataset_kind
        AND release_key = $parent.release_key AND state = 'active')[0] AS release
FROM time_active_authority
WHERE tenant = $tenant AND ($kind = NONE OR dataset_kind = $kind)
ORDER BY dataset_kind ASC;
