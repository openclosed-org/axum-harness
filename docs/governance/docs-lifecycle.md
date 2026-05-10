# Documentation Lifecycle

Keep tracked docs small and durable.

`docs/` is not the agent memory layer. Agents MUST NOT create new tracked docs files or directories without explicit user approval for the exact path and purpose.

## Keep Tracked

Tracked `docs/**` should be reserved for:

1. current operator and developer guidance
2. durable architecture decisions
3. minimal template-adoption guidance
4. a small amount of stable governance guidance
5. historical notes worth preserving as project history

Tracked `docs/**` should not contain vocabulary catalogs, status snapshots, gate transcripts, SDD drafts, agent behavior rules, or ordinary implementation progress notes.

Historical, superseded, deprecated, or target-state ADRs belong in `docs/archive/adr/**` instead of the default ADR path.

## Governance Memory

Use `docs/governance/out-of-scope/**` for rejected default directions that are stable enough to prevent repeated relitigation.

These files are not runtime evidence. They record why a direction should not enter the default repository path unless stated reconsideration criteria are met.

Examples:

1. treating metadata as proof
2. requiring app shells for backend-core work
3. defaulting every service to an independent process
4. claiming production readiness without matching evidence

## Move To Archive

Use `docs/archive/**` when material still has historical value, but is no longer current-state guidance.

Examples:

1. launch notes
2. old execution plans kept only for project history

## Keep In `docs/_local/`

Use `docs/_local/` for material that should not become shared documentation debt:

1. one-off refactor backlogs
2. temporary execution checklists
3. scratch notes and personal planning
4. exploratory drafts that have not become ADRs or stable guidance
5. one-time admission evidence snapshots, gate transcripts, or phase completion reports

`docs/_local/` is the gitignored maintainer workspace for active planning material that stays outside the shared docs flow.
If you need lightweight local revision tracking, use frontmatter metadata or explicit revision notes inside the document.

## Promotion Rule

Move something out of `_local` only if it becomes one of these:

1. reusable onboarding guidance
2. stable operator workflow
3. durable contributor rule
4. architecture decision with long-term value
5. template initialization or migration guidance

Do not promote a document only because it records a successful gate run or active development checkpoint. Gate results become stale quickly; keep them in the handoff, PR, local execution notes, or CI logs instead of the shared docs entry flow.

## GitHub Discussions

GitHub Discussions are useful for design rationale, but they are not current-state documentation by default.

Promote a discussion into tracked docs only when:

1. the decision still matches current executable sources
2. target-state language has been separated from current behavior
3. evidence level is clear: `declared`, `checked`, `tested`, or `proven`
4. the promoted text is shorter than the discussion and safe for agents to read as current guidance

If a discussion remains useful but not current, reference it as historical rationale instead of copying it into default docs.
