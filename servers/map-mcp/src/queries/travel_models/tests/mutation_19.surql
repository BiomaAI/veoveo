UPDATE ONLY $task SET request.input.request.identity.authority.tenant = 'other', result.payload.structuredContent.created_at = 'malformed' RETURN NONE;
