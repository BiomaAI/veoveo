DELETE media_usage
WHERE recorded_at < $cutoff RETURN BEFORE;
