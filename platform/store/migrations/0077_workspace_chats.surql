-- Workspace owns shared conversation state independently of agent wakes/episodes.
DEFINE TABLE workspace_chat SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON workspace_chat TYPE uuid;
DEFINE FIELD tenant ON workspace_chat TYPE record<tenant>;
DEFINE FIELD work_context ON workspace_chat TYPE record<work_context>;
DEFINE FIELD owner ON workspace_chat TYPE record<principal>;
DEFINE FIELD title ON workspace_chat TYPE string ASSERT string::len($value) IN 1..=200;
DEFINE FIELD initial_title ON workspace_chat TYPE string;
DEFINE FIELD archived ON workspace_chat TYPE bool DEFAULT false;
DEFINE FIELD members_can_invite ON workspace_chat TYPE bool DEFAULT false;
DEFINE FIELD revision ON workspace_chat TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD sequence ON workspace_chat TYPE int DEFAULT 1 ASSERT $value > 0;
DEFINE FIELD created_at ON workspace_chat TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON workspace_chat TYPE datetime DEFAULT time::now();
DEFINE INDEX workspace_chat_context ON workspace_chat FIELDS tenant, work_context, updated_at;

DEFINE TABLE workspace_member SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON workspace_member TYPE uuid;
DEFINE FIELD chat ON workspace_member TYPE record<workspace_chat>;
DEFINE FIELD principal ON workspace_member TYPE record<principal>;
DEFINE FIELD active ON workspace_member TYPE bool DEFAULT true;
DEFINE FIELD joined_at ON workspace_member TYPE datetime DEFAULT time::now();
DEFINE INDEX workspace_member_identity ON workspace_member FIELDS chat, principal UNIQUE;
DEFINE INDEX workspace_member_inbox ON workspace_member FIELDS principal, active, chat;

DEFINE TABLE workspace_invitation SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON workspace_invitation TYPE uuid;
DEFINE FIELD chat ON workspace_invitation TYPE record<workspace_chat>;
DEFINE FIELD inviter ON workspace_invitation TYPE record<principal>;
DEFINE FIELD invitee ON workspace_invitation TYPE record<principal>;
DEFINE FIELD state ON workspace_invitation TYPE 'pending' | 'accepted' | 'declined' | 'revoked' DEFAULT 'pending';
DEFINE FIELD created_at ON workspace_invitation TYPE datetime DEFAULT time::now();
DEFINE FIELD expires_at ON workspace_invitation TYPE datetime;
DEFINE INDEX workspace_invitation_inbox ON workspace_invitation FIELDS invitee, state, expires_at;
DEFINE INDEX workspace_invitation_chat ON workspace_invitation FIELDS chat, state;

DEFINE TABLE workspace_message SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON workspace_message TYPE uuid;
DEFINE FIELD chat ON workspace_message TYPE record<workspace_chat>;
DEFINE FIELD author ON workspace_message TYPE record<workspace_member>;
DEFINE FIELD text ON workspace_message TYPE string ASSERT string::len($value) IN 1..=32768;
DEFINE FIELD reply_to ON workspace_message TYPE option<record<workspace_message>>;
DEFINE FIELD sequence ON workspace_message TYPE int ASSERT $value > 0;
DEFINE FIELD created_at ON workspace_message TYPE datetime DEFAULT time::now();
DEFINE INDEX workspace_message_order ON workspace_message FIELDS chat, sequence UNIQUE;

DEFINE TABLE workspace_event SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON workspace_event TYPE array;
DEFINE FIELD chat ON workspace_event TYPE record<workspace_chat>;
DEFINE FIELD sequence ON workspace_event TYPE int ASSERT $value > 0;
DEFINE FIELD kind ON workspace_event TYPE 'created' | 'message' | 'invitation' | 'membership' | 'settings';
DEFINE FIELD target ON workspace_event TYPE record<workspace_chat | workspace_message | workspace_invitation | workspace_member>;
DEFINE FIELD created_at ON workspace_event TYPE datetime DEFAULT time::now();
DEFINE INDEX workspace_event_order ON workspace_event FIELDS chat, sequence UNIQUE;

-- The caller has evaluated current Work Context policy. Bind that decision to
-- its exact stored context, then enforce identity and chat membership here.
DEFINE FUNCTION fn::workspace_authority($authority: object) -> bool {
    LET $context = SELECT * FROM ONLY $authority.work_context;
    LET $principal = SELECT * FROM ONLY $authority.principal;
    RETURN $context != NONE AND $principal != NONE
        AND $principal.enabled = true AND $principal.kind = 'user'
        AND $principal.tenant = $authority.tenant
        AND $authority.tenant.enabled = true
        AND $context.tenant = $authority.tenant
        AND crypto::sha256(<string> [$context.policy_revision, $context.memberships, $context.output_policy]) = $authority.context_digest;
};

DEFINE FUNCTION fn::workspace_member($authority: object, $chat: record<workspace_chat>) -> bool {
    RETURN fn::workspace_authority($authority)
        AND $chat.tenant = $authority.tenant
        AND $chat.work_context = $authority.work_context
        AND count(SELECT id FROM workspace_member
            WHERE chat = $chat AND principal = $authority.principal AND active = true LIMIT 1) = 1;
};

