SELECT * FROM $key WHERE state.owner = $owner AND state.viewerActor = $actor AND ($live_session = NONE OR state.sessionId = $live_session) AND ($viewer = NONE OR state.viewerInstanceId = $viewer);
