# Metadata As Proof

## Decision

Do not treat YAML, platform models, service models, manifests, generated artifacts, or prose docs as proof of runtime behavior.

## Why This Is Out Of Scope

This repository uses metadata heavily, but metadata is a declaration layer. It can describe intended ownership, topology, deployable shape, commands, events, queries, consistency intent, and idempotency intent.

It cannot prove that the behavior exists or that the implementation respects the declaration.

Treating metadata as proof causes the most dangerous drift pattern in this repository: agents copy a target-state declaration into current-state docs or code plans and silently skip executable evidence.

## Reconsideration Criteria

Do not reconsider this as a default rule. A stronger claim can only be made for a specific path when validators, drift checks, tests, gates, scripts, command output, or runtime evidence support it.

## Related Guidance

1. `AGENTS.md`
2. `docs/architecture/north-star.md`
3. `docs/architecture/harness-philosophy.md`
4. `agent/codemap.yml`
