# Backend-Core Requires Frontend Repository

## Decision

Do not make external frontend repository package managers, UI libraries, desktop/mobile shells, admin UI work, or browser e2e checks prerequisites for backend-core commands.

## Why This Is Out Of Scope

The repository is backend-only. Frontend shells consume published contracts such as OpenAPI or SDK artifacts from separate repositories; they must not become required for root backend development or backend verification.

If frontend dependencies leak into backend-core commands, backend-only adopters inherit unnecessary UI/toolchain complexity.

## Reconsideration Criteria

Only add a frontend-repository requirement to a backend task when the task explicitly changes an external contract consumed by that repository and the agreed evidence requires downstream verification.

Default backend-core gates must remain independent.

Use `just audit-backend-core strict` to verify the root backend command surface stays free of frontend-repository requirements.

## Related Guidance

1. `AGENTS.md`
2. `docs/README.md`
3. `docs/contracts/README.md`
