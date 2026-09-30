SELECT day, class, outcome, count FROM audit_daily
WHERE partition = $partition AND count > 0
    AND day >= $from AND day < $until
    AND ($cursor_day = NONE OR day < $cursor_day
        OR (day = $cursor_day AND class > $cursor_class)
        OR (day = $cursor_day AND class = $cursor_class AND outcome > $cursor_outcome))
ORDER BY day DESC, class ASC, outcome ASC LIMIT $limit;
