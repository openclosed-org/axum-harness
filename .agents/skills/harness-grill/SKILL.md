---
name: harness-grill
description: >
  Alignment workflow for ambiguous harness plans, architecture choices, scope, and terminology.
  Use when requirements are fuzzy, terms are overloaded, a decision affects multiple ownership boundaries,
  or the user asks to stress-test a plan before implementation.
---

# Harness Grill

Use this skill to reach shared understanding before code or docs change.

Workflow skills guide process. Ownership skills still decide writable boundaries.

## Process

1. Read `AGENTS.md`, `agent/codemap.yml`, `docs/architecture/north-star.md`, and `docs/architecture/harness-philosophy.md`.
2. Explore executable sources when they can answer the question. Do not ask the user to restate facts the codebase can show.
3. Walk the decision tree one question at a time.
4. For each question, provide your recommended answer and why.
5. Use `declared`, `checked`, `tested`, and `proven` precisely.
6. Record unresolved vocabulary in `docs/_local/**` only when useful, without promoting it into tracked docs by default.
7. Offer an ADR only when the decision is hard to reverse, surprising without context, and a real trade-off.

## Challenge Rules

1. If user wording conflicts with `AGENTS.md`, `agent/codemap.yml`, accepted ADRs, or executable evidence, call out the conflict directly.
2. If a claim conflicts with executable evidence, surface the mismatch and ask which should change.
3. If the plan mixes current state, target state, file maps, and roadmap, recommend splitting before implementation.
4. If optional lanes are becoming backend-core prerequisites, flag drift.

## Stop Condition

Stop when the next implementation step, owner boundary, evidence expectation, and out-of-scope set are clear enough to hand to an ownership skill or agent brief.
