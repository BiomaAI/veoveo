UPDATE ONLY $task SET request.input.request.identity.actor.tenant = NONE, result.payload.structuredContent.created_at = 'malformed' RETURN NONE;
