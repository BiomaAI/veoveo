# Policy Instructions

Keep evaluation pure and use the canonical gateway policy types. This component
authenticates nobody and supplies no current-state cache or transport. Its callers
must prove current identity, session/grant state, Work Context and control-plane
freshness before they can treat an Allow decision as action authority.

Preserve one evaluator for gateway and background workers. Add policy cases to the
owning fixture suite whenever a rule changes. Do not import gateway, agent runtime,
provider SDKs or analytics to evaluate an action.
