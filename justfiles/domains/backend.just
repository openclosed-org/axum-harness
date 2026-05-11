# Backend command surface.
# Keeps primary, optional, secondary, and alternative backend evidence lanes named
# separately from aggregate verification and lifecycle gates.

_primary_packages := '-p kernel -p platform -p runtime -p contracts_api -p contracts_events -p contracts_errors -p adapter-telemetry-otel -p adapter-telemetry-tracing -p storage_turso -p user-service -p tenant-service -p counter-service -p event-bus -p commercial -p web-bff -p data -p observability'
_auth_optional_packages := '-p contracts_auth -p adapter-google-backend -p auth-service -p authz'
_secondary_packages := '-p pingora-gateway -p outbox-relay-worker -p indexer-worker -p projector-worker -p outbound-delivery-worker -p scheduler-worker -p sync-reconciler-worker -p notification -p bot-channel -p governance -p ai -p platform-validator -p dependency-validator -p contract-drift-detector -p topology-validator -p security-validator -p observability-validator -p platform-generator'
_alternative_fmt_packages := '-p tenant-service -p user-service -p counter-service -p web-bff -p storage_surrealdb'
_alternative_packages := '-p tenant-service -p user-service -p counter-service -p web-bff -p storage_surrealdb --no-default-features --features web-bff/surrealdb,storage_surrealdb/http'

# canonical entrypoint: default backend-core low-cost static bundle
check-backend-primary:
    cargo fmt {{_primary_packages}} -- --check
    cargo clippy {{_primary_packages}} --all-targets -- -D warnings
    cargo check {{_primary_packages}}

# 可选 auth lane 静态检查（不阻塞默认主链）
verify-auth-optional:
    cargo fmt {{_auth_optional_packages}} -- --check
    cargo clippy {{_auth_optional_packages}} --all-targets -- -D warnings
    cargo check {{_auth_optional_packages}}

# 次级 backend / governance 静态检查
verify-backend-secondary:
    cargo fmt {{_secondary_packages}} -- --check
    cargo clippy {{_secondary_packages}} --all-targets -- -D warnings
    cargo check {{_secondary_packages}}

# 备选 lane 静态检查
verify-backend-alternative:
    cargo fmt {{_alternative_fmt_packages}} -- --check
    cargo clippy {{_alternative_packages}} --all-targets -- -D warnings
    cargo check {{_alternative_packages}}

# backend experiments: compile drift across all backend tiers
verify-backend-experiments:
    cargo check {{_primary_packages}} {{_auth_optional_packages}} {{_secondary_packages}} {{_alternative_packages}}

# backend experiments stable-only clippy sweep
lint-backend-experiments:
    cargo clippy {{_primary_packages}} {{_auth_optional_packages}} {{_secondary_packages}} {{_alternative_packages}} --all-targets -- -D warnings

# primary backend coverage lane
coverage-backend-primary:
    cargo llvm-cov {{_primary_packages}} --lcov --output-path lcov.info

# 默认后端主链验证：counter-service + tenant-service + web-bff
test-backend-primary:
    cargo test -p kernel -p platform -p runtime -p contracts_api -p contracts_events -p contracts_errors -p adapter-telemetry-otel -p adapter-telemetry-tracing -p storage_turso -p user-service -p tenant-service -p counter-service -p event-bus -p web-bff -p data -p observability -- --test-threads=4

# 可选 auth lane 验证：auth-service + authz + auth contracts/adapters
test-auth-optional:
    cargo test -p contracts_auth -p adapter-google-backend -p auth-service -p authz

# 次级 backend / governance 测试
test-backend-secondary:
    cargo test -p pingora-gateway -p outbox-relay-worker -p indexer-worker -p projector-worker -p outbound-delivery-worker -p scheduler-worker -p sync-reconciler-worker -p notification -p bot-channel -p governance -p ai -p platform-validator -p dependency-validator -p contract-drift-detector -p topology-validator -p security-validator -p observability-validator -p platform-generator -- --test-threads=4

# 备选 lane 测试
test-backend-alternative:
    cargo test -p tenant-service -p user-service -p counter-service -p web-bff -p storage_surrealdb --no-default-features --features web-bff/surrealdb,storage_surrealdb/http -- --test-threads=4

# Local SurrealDB server integration test. Requires an external server, e.g.:
# surreal start --no-banner --log warn --user root --pass root --default-namespace axh --default-database main memory
test-surrealdb-local:
    cargo test -p storage_surrealdb --no-default-features --features http --test external_http -- --ignored --test-threads=1

# Local BFF SurrealDB integration test. Requires the same external server as test-surrealdb-local.
test-surrealdb-bff-local:
    cargo test -p web-bff --no-default-features --features web-bff/surrealdb --test surrealdb_tenant_store_test -- --ignored --test-threads=1

# SurrealDB SDK lane is explicit because it compiles the heavy upstream Rust SDK.
test-surrealdb-sdk-experimental:
    cargo test -p storage_surrealdb --no-default-features --features sdk -- --test-threads=4
