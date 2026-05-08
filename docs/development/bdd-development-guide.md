# BDD Development Guide

This repository uses BDD as an invariant-first development discipline, not as a requirement that every test use Cucumber or Gherkin.

BDD in this harness means:

```text
Describe observable behavior before internal structure.
Name the boundary and invariant before adding features.
Every capability claim must eventually resolve to executable tests, CI gates, or an explicit docs-only status.
```

## Required Questions

Before implementation starts, answer:

```text
Who acts, and under what verified identity?
Which tenant and resource are in scope?
What action is attempted?
What must be allowed?
What must be rejected?
How does state change?
How are events persisted?
How does retry or recovery behave?
What audit evidence remains?
```

If these questions cannot be answered, the work is not ready for implementation.

## Evidence Status

Use these labels in docs and status matrices:

```text
implemented   code exists and executable tests/gates cover the behavior
partial       code exists but acceptance, negative, or recovery coverage is incomplete
experimental  exploratory path; do not present as secure/stable default
missing       required capability has no meaningful implementation
unclear       evidence has not been checked yet
```

Use these evidence levels precisely:

```text
declared  docs, metadata, manifests, or model files state intent
checked   schema validation, static validation, typecheck, drift check, or boundary check inspected structure
tested    automated tests exercised behavior
proven    an executed gate or runtime evidence supports the claimed invariant
```

Docs are not proof. YAML is not proof. Generated artifacts are only evidence when regenerated from current sources and checked for drift.

## Scenario Template

Each behavior scenario should include:

```md
## Capability: <name>

### Invariant

<the property that must always hold>

### Scenario: <happy path>

Given <preconditions>
And <identity / tenant / resource state>
When <action>
Then <observable result>
And <state change>
And <event / audit / projection result>

### Scenario: <negative path>

Given <preconditions>
When <invalid action>
Then <request is rejected>
And <no forbidden state change occurs>
And <no forbidden outbox event occurs>
And <security audit evidence exists, if applicable>

### Evidence

- Code:
- Tests:
- Gates:
- Docs:
```

## Required Scenario Categories

New services, workers, adapters, or security-sensitive modules must cover the relevant categories below.

```text
Authn:
  missing token rejected
  malformed token rejected
  expired token rejected
  wrong issuer rejected
  wrong audience rejected
  unknown kid rejected
  alg=none rejected
  dev headers rejected in prod
  default secret rejected in prod

Tenant isolation:
  tenant_id is not trusted from request body
  tenant_id is not trusted from ordinary headers
  tenant scope comes from verified identity + membership
  cross-tenant reads are rejected
  cross-tenant writes are rejected
  repository access cannot bypass tenant scope

AuthZ:
  route-level permission
  resource-level permission
  domain invariant permission
  mock authz cannot boot in prod
  allow-all policy cannot boot in prod

Idempotency:
  same key + same mutation returns stable result
  same key + different mutation conflicts
  retry after timeout does not duplicate mutation
  retry after worker lag does not duplicate outbox event

Outbox:
  state mutation and outbox event share one causal boundary
  mutation without outbox event is impossible
  outbox event without committed mutation is impossible
  relay can resume
  duplicate delivery is safe

Worker recovery:
  crash before publish
  crash after publish before mark
  crash after mark before checkpoint
  duplicate event delivery
  stale checkpoint
  rebuild from empty read model

Audit:
  security-relevant denial is auditable
  business mutation is auditable where required
  audit event is tenant-scoped
  audit query cannot cross tenant
  sensitive fields are redacted

Production config safety:
  dev secret rejected in prod
  dev headers rejected in prod
  permissive CORS rejected in prod
  mock authz rejected in prod
  unsafe persistence rejected in prod where applicable
```

## Definition Of Done

A feature is not done when the happy path works. A feature is done only when:

```text
1. BDD scenarios exist or the work is explicitly docs/tooling-only.
2. Happy path tests pass.
3. Negative tests pass.
4. Tenant boundary is tested if tenant data is involved.
5. AuthZ boundary is tested if protected resources are involved.
6. Idempotency is tested if mutation is retryable.
7. Outbox/audit behavior is tested if state changes matter.
8. Production config safety is tested if config is involved.
9. Docs status is accurate.
10. CI or local gate covers the relevant test class.
```

## Tooling Guidance

Default to ordinary Rust tests for executable acceptance and invariant tests. Use Cucumber/Gherkin only when natural-language scenarios add real value.

Useful test styles:

```text
cargo test / cargo nextest for Rust execution
plain integration tests for most acceptance tests
rstest for table-driven negative tests
proptest for input-space invariants
cargo-deny and repo-tools gates for supply-chain and repo-control evidence
```

The project must not require Cucumber for every test. BDD is a design discipline first and a test framework second.
