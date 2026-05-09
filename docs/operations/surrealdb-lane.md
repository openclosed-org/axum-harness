# SurrealDB Database Provider

> Scope: SurrealDB is a first-class external database provider for services that
> opt into it through platform profiles and SOPS-injected runtime configuration.
> The upstream Rust SDK remains optional; the default provider uses the external
> HTTP server path so local, single-VPS, and k3s deployments all use the same DB
> server model.

## Model

SurrealDB is treated like Postgres or any other external database server:

1. local development and single-VPS deployment use Podman/Docker Compose
2. k3s deployment uses a StatefulSet, Service, PVC, and SOPS-managed Secret
3. GitHub CI starts SurrealDB with `surrealdb/setup-surreal@v2`
4. `storage_surrealdb` defaults to an external-server HTTP transport
5. the upstream Rust `surrealdb` SDK is allowed only behind the explicit `sdk` feature

This keeps default backend development fast and keeps SurrealDB behavior tests
close to the deployment model.

## Local Install

Install a pinned CLI/server version with the official Unix installer:

```bash
curl -sSf https://install.surrealdb.com | sh -s -- --version v3.0.5 "$HOME/.local/bin"
"$HOME/.local/bin/surreal" version
```

On macOS, Homebrew is also supported, but it follows the tap's available
formula version rather than pinning this repository's tested lane version:

```bash
brew install surrealdb/tap/surreal
surreal version
```

The Homebrew formula installs one executable that contains both the CLI and the
database server.

## Local Server

Start a local development server through the compose profile:

```bash
SURREALDB_USER=root SURREALDB_PASS=root \
podman compose -f infra/docker/compose/core.yaml --profile surrealdb up -d surrealdb
```

For direct CLI debugging, use the same namespace/database defaults:

```bash
surreal start --no-banner --log warn --user root --pass root \
  --default-namespace axh --default-database main memory
```

The default local endpoint is `http://localhost:8000`.

## Runtime Configuration

Runtime configuration is injected through platform profiles and SOPS-managed
secrets, not `.env.example` files.

Non-secret defaults are declared in:

1. `platform/model/resources/surrealdb.yaml`
2. `platform/model/environments/*.yaml`
3. `platform/model/topologies/*.yaml`

Secret values belong in SOPS-encrypted Kubernetes Secret files under
`infra/security/sops/<env>/`. The expected dev secret name is
`surrealdb-secrets` and it must define:

1. `SURREALDB_USER`
2. `SURREALDB_PASS`
3. `SURREALDB_NS`
4. `SURREALDB_DB`

The Web BFF selects the repository provider for `tenant-service`,
`counter-service`, and `user-service` with:

```text
APP_STORE_PROVIDER=turso|surrealdb
APP_SURREALDB_URL=http://127.0.0.1:8000
APP_SURREALDB_NS=axh
APP_SURREALDB_DB=main
APP_SURREALDB_USER=root
APP_SURREALDB_PASS=<from SOPS>
APP_SURREALDB_TENANT_SCOPE=platform
```

## Verification

Default backend-core gates do not require SurrealDB:

```bash
just check-backend-primary
just test-backend-primary
```

The SurrealDB database provider lane is path-scoped and uses the external-server
HTTP transport:

```bash
just verify-backend-alternative
just test-backend-alternative
```

The local external-server integration test is explicit because it requires a
running SurrealDB server:

```bash
surreal start --no-banner --log warn --user root --pass root \
  --default-namespace axh --default-database main memory
just test-surrealdb-local
```

The SDK lane is intentionally explicit because it compiles the upstream
`surrealdb` Rust crate:

```bash
just test-surrealdb-sdk-experimental
```

## Migrations

`storage_surrealdb::migration_dry_run()` returns versioned SurrealQL migration
statements without applying them. Apply migrations only through an explicit
admin path using `SurrealAdminMarker::unsafe_admin()`.

Current versions:

1. `0001_tenant_tables`
2. `0002_user_tenant_bindings`
3. `0003_graph_and_live_boundaries`

## Backup

Use the official CLI against the external server:

```bash
surreal export --conn http://localhost:8000 --ns axh --db main --user root --pass root backup.surql
```

## Restore

Restore with the official CLI:

```bash
surreal import --conn http://localhost:8000 --ns axh --db main --user root --pass root backup.surql
```

After restore, run a tenant-scoped verification query through the typed API.
The adapter exposes `storage_surrealdb::restore_verification_query()` for this
purpose. Do not verify restore by issuing raw tenant-string rewritten SurrealQL.

## Tenant Boundary Rules

Tenant code must use `TenantQueryOperation` through `SurrealDbPort::tenant_query`.
Raw SurrealQL requires `SurrealAdminMarker::unsafe_admin()` and is reserved for
admin tasks such as migrations, backup/restore verification setup, or operator
diagnostics.

Graph traversal and live query operations include tenant predicates at the edge,
target, and source/table boundary. Tests for these generated statements are the
minimum regression evidence for this provider.

Tenant isolation uses the adapter-owned `tenant_scope` field. Service tables can
still use a business `tenant_id` field where that is part of their domain model.
