SELECT type::string(record::id(id)) AS value FROM recording
             WHERE tenant = $tenant AND $clearance CONTAINSALL labels
             AND (string::contains(string::lowercase(type::string(record::id(id))), $needle)
                  OR string::contains(string::lowercase(recording_key), $needle))
             ORDER BY value ASC LIMIT $limit;
