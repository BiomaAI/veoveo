# {Platform} Connector Recipe

<!--
Frontmatter for the catalog tooling. Keep every field.
name: {kebab-case-platform}
offering: official | official-beta | official-preview | community
transport: remote | stdio | both
auth: oauth | api-key | service-account | none
admin_gated: yes | no
verified: YYYY-MM-DD
governed_upstream: yes | no
-->

One paragraph on what this platform adds to an installation, stated as
outcomes in the operator's domain. Name the Veoveo servers it pairs with.
If the server is community maintained, say so in the first sentence.

## Prerequisites

Accounts, licenses, tokens, admin enablement, and local tooling the agent
must confirm before installing. Name the exact credential and where it
comes from.

## Install

The Claude Code path first, as one copyable command. Follow with a generic
MCP client JSON block for other hosts. Pin versions where the vendor
publishes them. Default to the read-only mode when the server offers one.

## Verify

A step the agent can actually run, with the expected output. List the tool
surface, then make one harmless read call and show what success looks like.

## Use With Veoveo

Three worked prompts that pair this platform's tools with Veoveo tools.
Each prompt names real tools on both sides and states what artifact,
recording, or feature layer the result becomes.

## Governed Upstream

Only when `governed_upstream: yes`. The gateway registration fragment for
`gateway.json`, the `mcp/bridges/stdio` bridge configuration when the
server ships as stdio, and the validate command. State which scopes the
profile entry grants.

## Notes

Version caveats, deprecated paths an agent might find in stale guides, and
the date-stamped state of anything in preview.
