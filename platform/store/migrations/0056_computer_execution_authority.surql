-- Every new accepted operation retains verified source context for current policy.
-- Pre-profile candidate journals cannot infer this evidence from TaskOwner.
DEFINE FIELD execution_authority ON computer_operation TYPE object FLEXIBLE;
