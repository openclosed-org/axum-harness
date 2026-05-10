# Agent Brief Format

An agent brief is a durable task contract for future issue, PRD, or AFK-agent workflows.

The original issue or discussion is context. The brief is the work contract.

## Principles

1. Describe behavior, not implementation steps.
2. Prefer stable interfaces and concepts over line numbers or brittle file paths.
3. State ownership and forbidden boundaries.
4. State evidence level and required gates.
5. Include explicit out-of-scope items to prevent gold-plating.

## Template

```markdown
## Agent Brief

**Category:** bug | enhancement | refactor | docs | tooling
**Summary:** one-line description

**Current behavior:**
Describe what happens now and the evidence level.

**Desired behavior:**
Describe the observable behavior after the work is complete.

**Causal boundary:**
contract | service | server | worker | platform | tooling | docs | cross-boundary

**Owned paths:**
- path or owner area

**Forbidden paths:**
- adjacent areas that must not be changed

**Key interfaces:**
- interface, command, route, gate, or model shape that matters

**Acceptance criteria:**
- [ ] specific, testable criterion

**Required evidence:**
- command, test, validator, gate, or review evidence

**Out of scope:**
- adjacent behavior not included
```

## Avoid

1. line-number instructions
2. stale implementation walkthroughs
3. broad rewrites without invariant reason
4. claims above available evidence
