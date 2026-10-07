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

<!-- veoveo:contract-compliance:start -->
Contract revision: 4
Catalog revision: 2

- C01: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C02: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C03: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C04: met — fixed catalog and exact typed reading lookup
- C05: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C06: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C07: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C08: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C09: met — owned scope, resource and reading ID types
- C10: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C11: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C12: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C13: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C14: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C15: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C16: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C17: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C18: met
- C19: met
- C20: met — supplied by the authenticated test host
- C21: met
- C22: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C23: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C24: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C25: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C26: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C27: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C28: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C29: pending — The independent modular fixture has not completed qualification for this requirement; installed authentication, deployment and service behavior are outside its current fixture coverage.
- C30: met — stateless protocol handler
- C31: met — hosted Discover and list checks qualify this synthetic fixture; installed authentication and service behavior require their own qualification
- C32: pending — shared typed docs support is wired; K01–K08 qualification is in progress
- C33: pending — Owner naming producers, consumers and installed qualification are pending.
<!-- veoveo:contract-compliance:end -->
