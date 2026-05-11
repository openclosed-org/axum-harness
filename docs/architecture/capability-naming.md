# Capability Naming

This document defines the repository naming rules for cross-cutting capabilities, provider choices, resource entities, topology bindings, and Kubernetes metadata.

The goal is not short names. The goal is to prevent ambiguous names from letting agents or humans mix capability state, topology, provider, resource identity, and secret binding into one string.

## 1. ECS Model

Use an ECS-style metadata model:

1. Entity names identify stable things.
2. Components attach facts to entities.
3. Systems consume those facts to validate, generate, deploy, or run.

Do not encode component values into entity names unless the entity is truly a different physical object.

Correct separation:

```text
entity: counter-db
capability: database
state: external_single_node
provider: database.turso-cloud
adapter: database.libsql-remote-adapter
secret: counter-db-credentials
topology: single-vps
```

Incorrect mixing:

```text
entity: counter-shared-turso-local-dev-db
```

## 2. Capability Key

A capability key names the abstract cross-cutting capability slot. It must not name a provider, topology, or runtime state.

Use `snake_case`.

Examples:

```text
database
cache
eventing
authn
authz
observability
backup
billing
entitlement
quota
usage_metering
invoice
tax
email
push
notification
referral
invite
coupon
telegram_bot
discord_bot
public_api
webhook
oauth_app
admin
audit
support
abuse
compliance
export
analytics
```

## 3. Capability State

Every runtime capability state must use the same five-state enum:

```text
disabled
local_mock
local_real
external_single_node
external_distributed
```

| State | Meaning |
|---|---|
| `disabled` | The capability is intentionally off. It must not silently fallback to a mock or local implementation. |
| `local_mock` | The capability is fake or synthetic and only valid for development/test. It must not produce production claims. |
| `local_real` | The capability has real local semantics, such as durable local file DB, in-process cache, local filesystem, or DB ledger. |
| `external_single_node` | The capability depends on an external process, container, hosted API, or managed service without claiming repository-level distributed/HA semantics. |
| `external_distributed` | The capability depends on external distributed infrastructure and the repository has explicit semantics and evidence for that claim. |

Rules:

1. A topology is not a state.
2. A provider is not a state.
3. A secret name is not a state.
4. A resource preset is not a state.
5. A provider being distributed internally does not automatically make repository behavior `external_distributed`.

## 4. Provider Key

Provider keys must include the capability key as a prefix.

Use this form:

```text
<capability>.<provider-or-technology>
```

Examples:

| Capability | Provider key |
|---|---|
| `database` | `database.embedded-libsql` |
| `database` | `database.sqlite-file` |
| `database` | `database.turso-cloud` |
| `database` | `database.surrealdb` |
| `cache` | `cache.moka` |
| `cache` | `cache.valkey` |
| `eventing` | `eventing.db-outbox-polling` |
| `eventing` | `eventing.nats` |
| `authn` | `authn.dev-headers` |
| `authn` | `authn.oidc` |
| `authz` | `authz.local-policy` |
| `authz` | `authz.openfga` |
| `billing` | `billing.disabled` |
| `billing` | `billing.creem` |
| `usage_metering` | `usage_metering.db-ledger` |
| `quota` | `quota.db-ledger` |
| `notification` | `notification.noop` |
| `observability` | `observability.stdout` |
| `observability` | `observability.opentelemetry` |

Why the prefix is required:

1. `moka` alone does not say whether it is cache, rate-limit state, or another in-process store.
2. `creem` alone does not say whether it is billing, tax, entitlement, invoice, or webhook integration.
3. `libsql` alone does not say whether it is application DB, outbox storage, audit storage, or local cache persistence.
4. Capability-prefixed keys reduce agent guesswork when searching, validating, or generating config.

## 5. Adapter Key

Adapter keys identify repository code that adapts a provider to a capability port. They should also be capability-prefixed.

Examples:

```text
database.libsql-local-adapter
database.libsql-remote-adapter
billing.creem-checkout-adapter
billing.creem-webhook-adapter
quota.db-ledger-adapter
usage_metering.db-ledger-adapter
authn.dev-headers-adapter
authn.oidc-adapter
```

Rules:

1. Provider key says what technology or vendor is used.
2. Adapter key says which repository adapter implements the seam.
3. A provider can have multiple adapters.
4. A capability can have multiple providers.
5. Business services must depend on stable ports or seams, not provider SDKs.

## 6. Resource Entity Name

Resource entity names identify a concrete logical resource instance. They must not encode provider or state unless that is the stable identity of the physical object.

Use `kebab-case`.

Examples:

```text
counter-db
counter-outbox
counter-read-model
commercial-ledger
audit-log
default-cache
billing-provider
email-provider
object-store
```

Avoid:

```text
turso
shared-db
local-real-db
single-vps-db
creem-test-billing
```

Resource entity names should answer: "What logical resource is this?" They should not answer: "How is it implemented today?"

## 7. Topology And Preset Names

Topology names describe deployment shape, not capability state.

Examples:

```text
local-dev
single-vps
k3s-staging
k3s-production
```

Rules:

1. `local-dev` can contain multiple capability states.
2. `single-vps` can contain `local_real`, `external_single_node`, and `disabled` capabilities at the same time.
3. `k3s-staging` does not automatically mean `external_distributed` for every capability.
4. Topology chooses bindings; it does not redefine capability state names.

Resource preset names describe a bundle within a topology. They are not capability states.

Examples:

```text
lite
standard
full
```

## 8. Secret And Config Naming

Secret names should describe what credentials they contain, not the runtime state or topology nickname.

Target naming:

```text
resource entity: counter-db
k8s secret / SOPS deployable name: counter-db-credentials
```

Rationale:

1. `counter-db` is the logical resource.
2. `counter-db-credentials` is the Kubernetes Secret / SOPS file containing credentials for that resource.
3. The name does not encode state, provider, or topology.

Provider-specific env vars may keep provider names because the provider API requires them or because the adapter owns them.

Examples:

```text
APP_DATABASE_CAPABILITY_STATE=external_single_node
APP_DATABASE_PROVIDER=database.turso-cloud
APP_DATABASE_RESOURCE=counter-db
APP_TURSO_URL=libsql://...
APP_TURSO_AUTH_TOKEN=...
```

Rules:

1. Generic env vars select the capability state and provider.
2. Provider-specific env vars configure the selected adapter.
3. Business code must not branch on provider-specific env vars.
4. Composition roots may read provider-specific env vars to build adapters.

## 9. Kubernetes Metadata

`metadata.name` should identify the Kubernetes object. It should not be overloaded with capability state, topology, or provider unless that object is explicitly provider-specific.

Recommended secret:

```yaml
metadata:
  name: counter-db-credentials
```

Recommended labels:

```yaml
metadata:
  labels:
    app.kubernetes.io/part-of: axum-harness
    app.kubernetes.io/managed-by: kustomize
    harness.openclosed.org/entity-type: secret
    harness.openclosed.org/capability: database
    harness.openclosed.org/capability-state: external_single_node
    harness.openclosed.org/provider: database.turso-cloud
    harness.openclosed.org/resource: counter-db
    harness.openclosed.org/environment: dev
    harness.openclosed.org/topology: k3s-dev
```

Deployable labels:

```yaml
metadata:
  labels:
    app.kubernetes.io/name: web-bff
    app.kubernetes.io/component: bff
    harness.openclosed.org/entity-type: deployable
    harness.openclosed.org/deployable: web-bff
```

Rules:

1. Labels may repeat model values for operational selection.
2. Labels are not the source of truth if they conflict with platform model and generated output.
3. `metadata.name` remains stable identity; labels carry mutable classification.

## 10. Evidence

This naming model is not proven by this document.

Evidence levels:

1. `declared`: this document and platform model state the rule.
2. `checked`: schemas and validators enforce the rule.
3. `tested`: tests or gates exercise migrated paths.
4. `proven`: operational evidence demonstrates the selected runtime behavior.

Use these terms precisely in handoffs and completion reports.
