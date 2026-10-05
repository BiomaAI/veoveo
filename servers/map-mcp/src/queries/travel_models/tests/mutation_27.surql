UPDATE ONLY $task SET result.payload.isError = true, result.payload.structuredContent.created_at = 'malformed' RETURN NONE;
