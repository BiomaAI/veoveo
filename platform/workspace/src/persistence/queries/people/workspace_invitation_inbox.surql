LET $invitations = SELECT
    { id: id, chat: chat, inviter: inviter, invitee: invitee, state: state,
      created_at: created_at, expires_at: expires_at } AS invitation,
    chat.title AS chat_title, inviter AS inviter
    FROM workspace_invitation WHERE invitee = $authority.principal
    AND state = 'pending' AND expires_at > time::now()
    AND chat.tenant = $authority.tenant AND chat.work_context = $authority.work_context
    LIMIT 100;
LET $principals = array::distinct(array::map($invitations, |$entry: object| $entry.inviter));
LET $names = fn::kernel::identity::principal_summaries_v1($authority.tenant, $principals);
RETURN array::map($invitations, |$entry: object| {
    LET $matches = array::filter($names, |$person: object| $person.id = $entry.inviter);
    LET $person = array::first($matches);
    RETURN IF type::is_object($person) THEN {
        invitation: $entry.invitation, chat_title: $entry.chat_title, inviter_name: $person.display_name,
    } ELSE {
        invitation: $entry.invitation, chat_title: $entry.chat_title, inviter_name: NONE,
    } END;
});
