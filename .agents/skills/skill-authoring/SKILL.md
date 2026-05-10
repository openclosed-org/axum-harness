---
name: skill-authoring
description: >
  Creates, edits, or reviews repository agent skills with triggerable descriptions,
  progressive disclosure, and boundary-safe instructions. Use when adding, editing,
  or reviewing .agents/skills/** or workflow skill documentation.
---

# Skill Authoring

Use this skill for repository skill changes.

Read `agent/skill-authoring.md` before editing skills.

## Rules

1. Decide whether the skill is an ownership skill or workflow skill.
2. Keep `SKILL.md` focused and short.
3. Make the description a trigger interface with `Use when` language.
4. Put detailed reference material in adjacent files.
5. Do not duplicate `AGENTS.md`, `north-star.md`, or accepted ADRs.
6. Do not write target-state claims as current behavior.
7. Ensure workflow skills defer to ownership boundaries.

## Review Checklist

1. The skill has one clear reason to exist.
2. The trigger description is specific.
3. Writable and forbidden paths are clear when relevant.
4. Evidence terms are precise.
5. References are one level deep where possible.
6. No generated or target-state path is treated as current executable behavior.
