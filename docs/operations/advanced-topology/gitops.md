# GitOps

> 目的：说明本仓库当前 GitOps/Flux 落点，以及它如何挂接到 `counter-service` 的工程横切链。
>
> 本文档不是 Flux 从零安装教程；它只描述当前代码树里已经存在的定义、它们的参考价值，以及仍未闭环的部分。

## 1. 核心结论

当前仓库已经存在 GitOps 主路径的真实文件落点：

1. `infra/gitops/flux/infrastructure/infrastructure.yaml`
2. `infra/gitops/flux/apps/api.yaml`
3. `infra/gitops/flux/apps/outbox-relay-worker.yaml`
4. `infra/gitops/flux/apps/projector-worker.yaml`

这些文件说明：

1. Flux 已经被当作 cluster profile 的默认 GitOps 交付方向建模。
2. SOPS 解密已经被挂进 Flux Kustomization。
3. `counter-service` 相关能力目前主要通过 `infra/k3s/overlays/dev`、`web-bff`、`outbox-relay-worker`、`projector-worker` 这条路径进入 GitOps；独立 `counter-service` deployable 仍只是未提升到默认路径的 declared/deferred 形态。
4. GitOps is not a required dependency for backend-core local development.
5. Cluster nodes do not compile first-party Rust code. GitOps should consume prebuilt artifacts/images produced by a developer machine or CI.

## 2. 当前真实文件落点

### 2.1 Flux 根分层

主要文件：

1. `infra/gitops/flux/infrastructure/infrastructure.yaml`
2. `infra/gitops/flux/infrastructure/*.yaml`
3. `infra/gitops/flux/apps/api.yaml`

按当前目录理解：

1. `infrastructure/` 负责按需启用底层依赖，如 SurrealDB、NATS、Valkey、MinIO；它们是 capability backends，不是所有环境的默认税。
2. `apps/` 负责应用或交付单元的 Kustomization。

这与 single-VPS resource preset 语义一致：`lite` 不默认启动分布式资源；`standard/full` 才按业务压力启用更重的 resource backends。

### 2.2 与 counter 参考链的真实挂接点

当前最关键的挂接点不是某个名为 `counter` 的 Flux 文件，而是：

1. `infra/k3s/overlays/dev/kustomization.yaml`
2. `infra/security/sops/dev/web-bff.enc.yaml`
3. `infra/security/sops/dev/outbox-relay-worker.enc.yaml`
4. `infra/security/sops/dev/projector-worker.enc.yaml`
5. `infra/security/sops/dev/counter-db-credentials.enc.yaml`
6. `infra/security/sops/dev/counter-service.enc.yaml`

这反映出当前真实状态：

1. counter 的同步主路径嵌在 `web-bff` 中。
2. `web-bff` 所在 dev overlay 已显式消费 `counter-db-credentials` secret，使 cluster 路径选择 DB capability state `external_single_node` 或 `external_distributed`，而不是本地 embedded DB。
3. counter 的异步发布路径已经有独立 `outbox-relay-worker` Flux app 与 dev overlay，当前 overlay 中显式保持 replicas=1。
4. `projector-worker` 已有独立 Flux app 与 dev overlay，当前 overlay 中显式保持 replicas=1。
5. `counter-db-credentials` secret 是 external DB capability state 的 secret source，为 `web-bff` 与这些独立 worker 提供共享数据库入口。
6. 独立 `counter-service` secrets 已预留，但 overlay 中仍注释掉了对应资源。

## 3. 当前已经确认的事实

通过现有 YAML 可以确认：

1. `api.yaml` 配置了 `decryption.provider: sops`。
2. `api.yaml` 使用 `secretRef.name: sops-age`，说明 Flux 期望在集群中持有 age key secret。
3. `infrastructure.yaml` 明确先于应用层落地基础依赖。
4. `infra/k3s/overlays/dev/kustomization.yaml` 是当前 `web-bff` 主链的 dev overlay 入口。
5. `infra/k3s/overlays/dev/outbox-relay-worker/kustomization.yaml` 是 `outbox-relay-worker` 的独立 dev overlay 入口。
6. `infra/k3s/overlays/dev/projector-worker/kustomization.yaml` 是 `projector-worker` 的独立 dev overlay 入口。
7. `infra/kubernetes/addons/surrealdb.yaml` 和 `infra/gitops/flux/infrastructure/infrastructure.yaml` 已声明 SurrealDB infrastructure landing point，但本仓库会话尚未测试 live Flux reconciliation。

## 4. 当前不能过度承诺的部分

这部分必须写清楚，否则文档会再次漂移。

当前还不能声称：

1. GitOps 路径已经把独立 `counter-service` deployable 提升为 checked/tested default path。
2. `infra/gitops/flux/apps/api.yaml` 中的所有 health checks、命名和 target resources 都已经与现状完全一致。
3. `outbox-relay-worker` 与 `projector-worker` 当前在 dev overlay 中都显式配置为 `replicas=1`；因此更需要先通过 `just sops-verify-counter-db-credentials dev` 和 `just verify-counter-delivery strict` 核实 external DB secret、overlay 和 Flux 路径没有漂移。
4. promotion、rollback、drift handling 已经通过一条统一且经验证的流水线完成。
5. K3s/K3d nodes 会或应该在集群内编译 first-party Rust code。
6. GitOps 当前已经证明 `k3s-ha`；K3d 和单节点/少节点 rehearsal 不能替代 3+ server-node HA evidence。

因此这份文档的正确定位是：

1. GitOps 结构已存在。
2. SOPS 解密已挂接。
3. counter 已经接入这条工程路径的一部分。
4. 但仍需要围绕 `counter-service` 继续补齐 deployable、promotion、drift、runbook 主链。

## 5. 默认理解路径

如果要理解当前 GitOps 如何服务后端默认主链，建议按这个顺序看：

1. `docs/operations/counter-service-reference-chain.md`
2. `infra/security/sops/templates/dev/*.yaml`
3. `infra/security/sops/dev/*.enc.yaml`
4. `infra/k3s/overlays/dev/kustomization.yaml`
5. `infra/k3s/overlays/dev/outbox-relay-worker/kustomization.yaml`
6. `infra/k3s/overlays/dev/projector-worker/kustomization.yaml`
7. `infra/gitops/flux/infrastructure/infrastructure.yaml`
8. `infra/gitops/flux/apps/api.yaml`
9. `infra/gitops/flux/apps/outbox-relay-worker.yaml`
10. `infra/gitops/flux/apps/projector-worker.yaml`

这样读的原因是：

1. 先看 counter 参考链，知道自己在追哪条主线。
2. 再看 secrets 和 overlay，知道应用清单实际从哪里拿配置。
3. 最后再看 Flux Kustomization，知道 Git 是如何驱动集群落地的。

## 6. 与 secrets 文档的关系

GitOps 文档和 `secret-management.md` 是同一条工程链上的相邻两环：

1. `secret-management.md` 关注 secrets 如何被编辑、加密、注入。
2. 本文关注这些加密产物如何被 overlay 和 Flux 消费。

两者共同服务 `counter-service` 的工程横切链，而不是两套独立体系。

## 7. 一句话结论

当前仓库已经把 Flux/SOPS/GitOps 放进 cluster profile 的后端工程方向；`web-bff`、`outbox-relay-worker`、`projector-worker` 都已接入 shared counter DB secret，但它们当前仍不应被当成“完全闭环的生产交付模板”。`counter-service` 的独立 deployable、共享 worker 可靠性语义，以及后续 promotion/drift 闭环仍需继续补齐。

GitOps 的正确目标是接收预构建应用 artifact/image、注入 SOPS-backed configuration，并协调按需资源；不是把集群节点变成 Cargo builder。
