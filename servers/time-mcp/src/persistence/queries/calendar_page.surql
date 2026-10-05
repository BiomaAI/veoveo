SELECT * FROM time_calendar_version
WHERE tenant = $tenant AND ($after_key = NONE OR calendar_key > $after_key OR (calendar_key = $after_key AND calendar_version < $after_version))
ORDER BY calendar_key ASC, calendar_version DESC
LIMIT $limit;
