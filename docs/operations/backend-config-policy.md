# Backend Configuration and Secrets Policy

> **硬规则**: 后端配置与密钥的 canonical cluster shape 是 `Kustomize + SOPS + age + Flux`。
> **`.env` 不得再作为后端 reference path 的第一入口。**

---

## Architecture Decision

后端配置和密钥管理采用以下统一路径：

1. **`Kustomize`** — 组织环境差异 (dev/staging/prod)
2. **`SOPS`** — 加密配置与密钥
3. **`age`** — 作为密钥机制
4. **`Flux`** — 负责 GitOps 解密与部署

后端二进制文件只消费标准环境变量，不感知 `.env` 文件。

---

## Why No `.env`

`.env` 文件不再作为后端参考入口，原因：

1. **单一声明入口** — 后端参考路径的敏感配置统一由 SOPS 管理
2. **环境同构** — dev/staging/prod 使用相同的配置路径，只是 overlay 不同
3. **GitOps 友好** — Flux 可以自动解密和应用加密密钥
4. **Agent 一致性** — 新 agent 进入仓库后不会优先找 `.env`
5. **生产对齐路径** — 从开发第一天起就使用接近集群交付的配置形状

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
│  Flux (GitOps Controller)               │
│                                         │
│  1. 读取 .enc.yaml 文件                 │
│  2. 使用 age 密钥解密（SOPS）          │
│  3. 应用 Kubernetes manifests           │
│  4. Secret 注入为 Pod 环境变量          │
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

## Local Development

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

唯一的非集群快速调试路径：

```bash
# sops exec-env 风格 — 不产生 .env 文件
just sops-run web-bff
# 等价于：
# sops exec-env infra/security/sops/dev/web-bff.enc.yaml -- cargo run -p web-bff

just sops-run outbox-relay-worker
just sops-run projector-worker
just sops-run counter-shared-db  # 仅用于检查 secret 形状，不直接启动二进制
just sops-run counter-service
```

**这是 cluster path 的派生辅助命令，不是新的配置声明入口。**

同时需要注意：

1. 当前默认后端运行形态仍以 `web-bff` 内嵌 `counter-service` 为主。
2. `counter-service` 自身的独立 secret 路径已预留，但不是默认运行主路径。

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
# 无集群
just sops-run web-bff

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
4. **使用 sops-run** — `just sops-run web-bff`

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

- [AGE-KEY-MANAGEMENT.md](./AGE-KEY-MANAGEMENT.md) — age 密钥管理详细指南
- [SOPS 官方文档](https://github.com/getsops/sops) — SOPS 使用
- [Flux SOPS 集成](https://fluxcd.io/flux/guides/mozilla-sops/) — GitOps 解密
