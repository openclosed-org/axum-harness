---
name: harness-architecture-review
description: >
  Finds architecture deepening opportunities using seams, module depth, deletion tests,
  evidence hierarchy, and repository ownership boundaries. Use when reviewing architecture,
  reducing coupling, improving testability, or evaluating whether a module, trait, adapter,
  worker, service, or boundary is too shallow.
---

# Harness Architecture Review

Use this skill to surface architecture friction before proposing refactors.

Workflow skills guide process. Ownership skills still decide writable boundaries.

## Process

1. Read `AGENTS.md`, `agent/codemap.yml`, `docs/architecture/north-star.md`, and accepted ADRs in the area.
2. Explore code and tests before judging architecture.
3. Identify where understanding requires bouncing across many shallow modules.
4. Apply the deletion test to suspected pass-through modules.
5. Check whether a seam has real variation, testing value, or topology-late value.
6. Check ownership boundaries before recommending changes.
7. Present candidates only. Do not edit first.

## Candidate Format

For each candidate, report:

1. files or owner areas involved
2. problem and evidence
3. why the current module or seam is shallow
4. proposed deeper interface in plain English
5. benefit in leverage, locality, and testability
6. ADR or north-star conflict, if any
7. risk and suggested verification

Ask which candidate to explore before designing interfaces or changing code.

## Repository-Specific Rules

1. A service library can be a semantic boundary without becoming a process boundary.
2. A Rust trait is not automatically a useful seam.
3. Platform metadata must not absorb service-local semantics.
4. Server handlers adapt protocols; they do not own domain rules.
5. Worker reliability claims require executable retry, checkpoint, dedupe, replay, and recovery evidence at the claimed level.
