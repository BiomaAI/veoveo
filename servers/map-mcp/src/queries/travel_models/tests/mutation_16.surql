UPDATE ONLY $task SET request.input.request.identity.actor.data_labels = ['secret'], result.payload.structuredContent.created_at = 'malformed' RETURN NONE;
