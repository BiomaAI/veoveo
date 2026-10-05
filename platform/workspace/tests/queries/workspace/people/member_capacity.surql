FOR $index IN 0..260 {
    LET $principal = CREATE ONLY type::record('principal', rand::uuid()) SET
        tenant = $tenant, issuer = 'https://capacity.test', subject = <string> $index,
        kind = 'user', display_name = string::concat('Member ', <string> $index), claims_hash = '';
    CREATE ONLY type::record('workspace_member', rand::uuid()) SET chat = $chat, principal = $principal.id;
};
