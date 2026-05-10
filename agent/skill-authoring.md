# Skill Authoring

Repository skills should be small, triggerable, and composable.

## Skill Types

**Ownership skills** define writable paths and responsibility boundaries, such as `service-agent` or `worker-agent`.

**Workflow skills** define how to perform an engineering activity, such as diagnosis, TDD, grilling, or architecture review.

Workflow skills guide process. Ownership skills still decide writable boundaries.

## Required Shape

Every `SKILL.md` must have frontmatter:

```yaml
---
name: skill-name
description: >
  Short capability summary. Use when specific triggers, file paths, or task types apply.
---
```

The description is the trigger interface. It must state what the skill does and when to use it.

## Writing Rules

1. Keep `SKILL.md` short enough to load frequently.
2. Move detailed references into adjacent files only when they are stable, scoped, and explicitly owned by the skill.
3. Do not duplicate `AGENTS.md`, `north-star.md`, or ADR doctrine.
4. Do not describe target state as current behavior.
5. Do not create a new source of truth for runtime behavior.
6. Link references one level deep when possible.
7. Add scripts only for deterministic operations, validation, formatting, or repeatable evidence capture.
8. Do not create vocabulary catalogs or status snapshots inside skills.

## Review Checklist

1. Description includes `Use when` triggers.
2. The skill has one clear responsibility.
3. Writable and forbidden paths are clear for ownership skills.
4. Workflow skills defer to ownership boundaries.
5. Evidence language uses `declared`, `checked`, `tested`, and `proven` precisely.
6. No time-sensitive claims or stale file maps are embedded.
