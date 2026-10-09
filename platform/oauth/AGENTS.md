# OAuth Client Assertion Instructions

Follow root AGENTS.md and this component's DESIGN.md. Keep this library pure RSA
client-assertion signing. HTTP discovery, file permissions, token caches, issuer
verification and domain authorization belong to callers. Keep RS256 fixed and
never log private keys or assertions. Use existing workspace pins and maintained
JWT mechanics. Qualify claims, signature, key selection, redaction and fresh jti
with focused Rust controls; installed authentication belongs to its owner.
