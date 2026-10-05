SELECT * FROM map_restriction WHERE tenant = $tenant AND cancelled_by = NONE AND valid_from <= $at
                AND (valid_until = NONE OR valid_until > $at)
                AND ($family = NONE OR affected_mobility_families CONTAINS $family)
             ORDER BY restriction_key ASC LIMIT $limit TIMEOUT 5s;
