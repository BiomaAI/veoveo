UPSERT ONLY $checkpoint SET
    versionstamp = math::max([versionstamp ?? 0, $cursor]),
    updated_at = $now RETURN NONE;
