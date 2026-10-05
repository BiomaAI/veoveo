IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
RETURN fn::kernel::identity::principal_summaries_v1($authority.tenant,
    (SELECT VALUE principal FROM workspace_member WHERE chat = $command.chat LIMIT 256));
