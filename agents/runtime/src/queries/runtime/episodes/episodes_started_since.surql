SELECT count() AS count FROM agent_episode WHERE agent = $agent AND started_at >= $since GROUP ALL;
