# Justfiles

`justfiles/` is the project action control surface behind the root `Justfile`.
It is not a script dump, planning area, or place for one-off historical commands.

## Current Layout

1. `core/` owns setup, development loop, build, and cleanup commands.
2. `quality/` owns verification, gates, and supply-chain checks.
3. `domains/` owns backend, contract, and platform domain command surfaces.
4. `ops/` owns secrets, deploy, runtime resources, and local topology profiles.
5. `agent/` owns template and agent-adjacent operations.

## Gate Classes

New or changed recipes must fit one class from `agent/architecture/gate-taxonomy.yml`:

1. `identity` — repository ontology, directory grammar, naming grammar, and agent context identity.
2. `source` — format, lint, compile, static quality, and source hygiene.
3. `architecture` — Clean, Hexagonal, DDD, dependency, import, and layer boundaries.
4. `behavior` — executable behavior, contract, integration, replay, and smoke tests.
5. `evidence` — coverage, mutation testing, JUnit, summaries, and reports.
6. `security` — secrets, env shape, SOPS, dependency policy, and supply-chain checks.
7. `topology` — local, VPS, Podman, systemd, Quadlet, K3d, K3s, and Multipass topology validation.
8. `agent` — codemap, routing, gate matrix, architecture YAML, and skill consistency.
9. `hygiene` — inventory, orphan or zombie candidates, stale docs, ignored file audit, and duplicate surface detection.
10. `release` — composed release, lifecycle, and P0 readiness gates.

## Naming

Use stable prefixes for new recipes:

1. `fmt-*`
2. `lint-*`
3. `check-*`
4. `validate-*`
5. `test-*`
6. `coverage-*`
7. `mutants-*`
8. `smoke-*`
9. `audit-*`
10. `render-*`
11. `deploy-*`
12. `gate-*`
13. `agent-*`

Existing accepted aliases such as `verify-*`, `typecheck`, and `boundary-check` may remain until a separate migration plan exists.

Do not add new recipes named like `run-all`, `do-check`, `my-test`, `test2`, `full`, `misc`, `temp`, or `verify-stuff`.

## Adding A Recipe

Before adding a recipe, identify:

1. class
2. cost level
3. scope
4. evidence output
5. whether it is safe for agent auto-run
6. whether CI may use it
7. whether it is release blocking

Deploy recipes must have a matching validate, dry-run, or smoke recipe. Dev convenience recipes must not be used as CI or release proof.

## Migration Rule

Do not delete, rename, or move existing recipes only to match the taxonomy. First produce a migration plan with compatibility expectations, owner, validation path, and deprecation window.
