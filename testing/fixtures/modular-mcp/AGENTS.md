# Modular MCP Fixture Instructions

Follow the repository and parent fixture instructions.

## Purpose

Qualify an independent server contract without adding domain definitions to core.

## Invariants

- Keep scope, reading ID and route vocabulary in the library's contract feature.
- Use synthetic grants and an isolated loopback listener. No installation credentials.
- The fixture declares only implemented resource behavior; no compliance marker grants authority.

## Build And Test

Run the fixture contract tests and `mcp/conformance` modular-server hosted test.
Resolve an independent contract consumer to check dependency isolation.

## Contract Compliance

Contract revision: 3

- C04: met — fixed catalog and exact typed reading lookup
- C09: met — owned scope, resource and reading ID types
- C18: met
- C19: met
- C20: met — supplied by the authenticated test host
- C21: met
- C30: met — stateless protocol handler
- C31: met — hosted Discover and list checks qualify this synthetic fixture; installed authentication and service behavior require their own qualification
- C32: pending — shared typed docs support is wired; K01–K08 qualification is in progress
