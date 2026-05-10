# Backend Configuration and Secrets Policy

> **硬规则**: 后端配置与密钥的 canonical secret shape 是 `SOPS + age`，并可挂接到 `sops-run`、单 VPS host env-file、或 `Kustomize + Flux`。
> **`.env` 不得再作为后端 reference path 的第一入口。**

---

## Architecture Decision

后端配置和密钥管理采用以下统一路径：

1. **`SOPS`** — 加密 deployable 配置与密钥
2. **`age`** — 作为密钥机制
3. **`sops-run`** — 本地进程注入，不生成 `.env`
4. **`sops-export-env`** — 单 VPS `systemd-binary` / `podman` 宿主机临时 env-file 注入
5. **`Kustomize + Flux`** — 集群路径解密与部署

后端二进制文件只消费标准环境变量，不感知 `.env` 文件。

---

## Why No `.env`

`.env` 文件不再作为后端参考入口，原因：

1. **单一声明入口** — 后端参考路径的敏感配置统一由 SOPS 管理
2. **环境同构** — dev/staging/prod 使用相同的配置路径，只是 overlay 不同
3. **GitOps 友好** — Flux 可以自动解密和应用加密密钥
4. **Agent 一致性** — 新 agent 进入仓库后不会优先找 `.env`
5. **生产对齐路径** — 从开发第一天起就使用接近部署 profile 的配置形状

---

## Configuration Flow

```
┌─────────────────────────────────────────┐
│  Git Repository                         │
│                                         │
│  infra/security/sops/                   │
│    templates/<env>/<deployable>.yaml   │  ← 模板（未加密）
│    <env>/<deployable>.enc.yaml         │  ← 加密密钥（SOPS + age）
│                                         │
│  infra/k3s/base/                        │
│    configmaps/<deployable>-config.yaml │  ← 公开配置（非敏感）
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│  Injection Boundary                     │
│                                         │
│  - sops-run process env                 │
│  - sops-export-env host env-file        │
│  - Flux/Kustomize Kubernetes Secret     │
└──────────────┬──────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────┐
│  Backend Binary (web-bff, worker, etc.)│
│                                         │
│  读取标准环境变量：                      │
│    SERVER_HOST, DATABASE_URL, etc.      │
│                                         │
│  不感知 .env、SOPS、或加密              │
└─────────────────────────────────────────┘
```

---

## Local Development And Single-VPS Profiles

### Local Cluster Validation Path

For the low-resource local Kubernetes profile, use the K3d gate surface:

```bash
just smoke-local-k3d
just gate-local-k3d
```

This path starts or reuses Colima + K3d, reconciles dev SOPS secrets directly into the cluster, applies `infra/kubernetes/addons/`, and verifies SurrealDB readiness plus pod-restart PVC persistence.

### K3s/GitOps Delivery Direction

使用仓库当前的 K3s overlay 路径跑服务，通过 Kustomize/Flux 注入配置：

```bash
# 应用加密密钥到集群
just sops-reconcile dev

# 部署应用
just deploy-prod dev
```

This is the K3s/GitOps delivery direction, not the local K3d smoke entrypoint.

### Quick Inner Loop (No Cluster)

推荐的本地进程调试路径：

```bash
# repo-tools secrets run 风格 — 不产生 .env 文件
just sops-run web-bff

just sops-run outbox-relay-worker
just sops-run projector-worker
just sops-run counter-shared-db  # 仅用于检查 secret 形状，不直接启动二进制
just sops-run counter-service
```

**这是 cluster path 的派生辅助命令，不是新的配置声明入口。**

### Single-VPS Binary Or Podman Resources

单 VPS `systemd-binary` 和 `podman` profiles 使用同一份 SOPS/age secrets，不使用 `.env`：

```bash
just sops-export-env web-bff dev systemd-binary /run/axum-harness/web-bff.env
just sops-export-env web-bff dev podman /run/axum-harness/web-bff.env
```

这会在宿主机写出 `0600` 临时 env-file。systemd 通过 `EnvironmentFile=` 消费；如果团队选择 first-party prebuilt application image，Podman 通过 `podman run --env-file`、Quadlet 或 systemd-managed container wiring 消费。应用容器默认不需要安装 `sops` 或 `age`。

运行环境不负责编译 first-party Rust code。开发机或 CI 产出 binary/prebuilt image，VPS、Podman runtime 和 K3s node 只负责接收 artifact、注入配置、启动、健康检查和回滚。

Podman 在单 VPS 默认用于官方资源容器，而不是用于在 VPS 上执行 `cargo build --release`。低资源 preset 可以不开启资源容器，或使用 embedded/in-process/local/managed backends。

同时需要注意：

1. 当前默认后端运行形态仍以 `web-bff` 内嵌 `counter-service` 为主。
2. `counter-service` 自身的独立 secret 路径已预留，但不是默认运行主路径。
3. SurrealDB 是当前仓库优先考虑的可选外部 DB server lane；Turso Cloud 是可选托管 libSQL 路径；PostgreSQL 不是 reference backend。

---

## Deployables and Their Secrets

### web-bff

| 类型 | 来源 | 示例 |
|---|---|---|
| 公开配置 | ConfigMap | SERVER_HOST, SERVER_PORT, RUST_LOG |
| 敏感配置 | SOPS Secret | JWT_SECRET, DATABASE_URL, CORS_ALLOWED_ORIGINS |

### outbox-relay-worker

| 类型 | 来源 | 示例 |
|---|---|---|
| 公开配置 | ConfigMap | OUTBOX_POLL_INTERVAL_MS, OUTBOX_BATCH_SIZE, OUTBOX_NATS_SUBJECT_PREFIX |
| 敏感配置 | SOPS Secret | OUTBOX_DATABASE_URL, OUTBOX_TURSO_AUTH_TOKEN, OUTBOX_NATS_URL |

### projector-worker

| 类型 | 来源 | 示例 |
|---|---|---|
| 公开配置 | ConfigMap | PROJECTOR_POLL_INTERVAL_MS, PROJECTOR_BATCH_SIZE, PROJECTOR_CHECKPOINT_PATH |
| 敏感配置 | SOPS Secret | PROJECTOR_DATABASE_URL, PROJECTOR_TURSO_AUTH_TOKEN |

### counter-service (Phase 1+)

| 类型 | 来源 | 示例 |
|---|---|---|
| 公开配置 | ConfigMap | RUST_LOG, NATS_SUBJECT_PREFIX |
| 敏感配置 | SOPS Secret | DATABASE_URL, TURSO_AUTH_TOKEN |

---

## Getting Started

### 1. 安装工具

```bash
mise install  # 包含 age + sops
```

### 2. 生成 age 密钥

```bash
just sops-gen-age-key
```

### 3. 更新 `.sops.yaml`

将公钥复制到 `.sops.yaml` 中对应环境的 recipients。

### 4. 创建加密密钥

```bash
just sops-encrypt-dev web-bff
just sops-encrypt-dev outbox-relay-worker
just sops-encrypt-dev projector-worker
just sops-encrypt-dev counter-shared-db
```

### 5. 运行服务

```bash
# 无集群进程注入
just sops-run web-bff

# 单 VPS host env-file 注入
just sops-export-env web-bff dev systemd-binary /run/axum-harness/web-bff.env
just sops-export-env web-bff dev podman /run/axum-harness/web-bff.env

# 有集群
just sops-reconcile dev
just deploy-prod dev
```

---

## File Structure

```
infra/security/sops/
├── .sops.yaml              # SOPS 规则文件（旧位置，保留兼容）
├── templates/              # 明文模板
│   ├── dev/
│   │   ├── web-bff.yaml
│   │   ├── outbox-relay-worker.yaml
│   │   ├── projector-worker.yaml
│   │   ├── counter-shared-db.yaml
│   │   └── counter-service.yaml
│   └── staging/
│       ├── web-bff.yaml
│       └── outbox-relay-worker.yaml
├── dev/                    # 加密密钥（dev）
│   ├── web-bff.enc.yaml
│   ├── outbox-relay-worker.enc.yaml
│   ├── projector-worker.enc.yaml
│   ├── counter-shared-db.enc.yaml
│   └── counter-service.enc.yaml
├── staging/                # 加密密钥（staging）
│   ├── web-bff.enc.yaml
│   └── outbox-relay-worker.enc.yaml
└── prod/                   # 加密密钥（prod）

.sops.yaml                  # 统一 SOPS 规则（根目录）
```

Secrets 操作入口统一通过 `repo-tools secrets ...` 或 `just sops-*` recipe，不再维护目录内 shell helper。

---

## Migration from `.env`

如果你的本地开发还在使用 `.env`：

1. **停止使用 `.env`** — 删除或移出 `.env` 文件
2. **生成 age 密钥** — `just sops-gen-age-key`
3. **创建加密密钥** — 参考模板，填入值，然后 `just sops-encrypt-dev <deployable>`
4. **本地进程使用 sops-run** — `just sops-run web-bff`
5. **单 VPS 使用 sops-export-env** — `just sops-export-env web-bff dev systemd-binary /run/axum-harness/web-bff.env`

---

## Troubleshooting

### "No matching key for encryption"

你的 age 公钥不在 `.sops.yaml` recipients 中。运行：
```bash
just sops-show-age-key
```
然后更新 `.sops.yaml`。

### "Decryption failed"

确保 SOPS 能找到你的 age 密钥：
```bash
export SOPS_AGE_KEY_FILE=~/.config/sops/age/key.txt
```

### 本地开发想用 `.env`

**不允许。** 使用 `just sops-run <deployable>` 代替。
这是当前仓库为后端主链维护的统一配置路径，不代表所有未来平台能力都已落地。

---

## See Also

- [secret-management.md](./secret-management.md) — 当前仓库 SOPS/age 文件落点与 profile 注入路径
- [SOPS 官方文档](https://github.com/getsops/sops) — SOPS 使用
- [Flux SOPS 集成](https://fluxcd.io/flux/guides/mozilla-sops/) — GitOps 解密
