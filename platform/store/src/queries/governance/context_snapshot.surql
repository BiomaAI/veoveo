LET $current = SELECT * FROM ONLY $context;
RETURN { context: $current, digest: crypto::sha256(<string> [$current.policy_revision, $current.memberships, $current.output_policy]) };
