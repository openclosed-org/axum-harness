# Counter Delivery Runbook

> 目的：为 `counter-service` 参考链提供最小但可执行的交付运行手册，覆盖 counter DB credentials 校验、worker/Flux 落点和当前 promotion 前置条件。

## 1. 适用范围

当前 runbook 覆盖 counter delivery 主链：

1. `web-bff` 通过 `counter-db-credentials` secret 指向 external DB capability state 的 libSQL/Turso 数据源。
2. `outbox-relay-worker` dev overlay + Flux app 已存在并默认启用。
3. `projector-worker` dev overlay + Flux app 已存在并默认启用。
4. staging overlay + Flux app 已创建（需 staging SOPS secrets 才可 deploy）。

它不声称已经覆盖独立 `counter-service` deployable、完整 rollback 自动化或 prod promotion 流水线。

## 2. 交付前检查

在把 counter delivery 视为可交付主链之前，至少执行：

```bash
just sops-verify-counter-db-credentials dev
just verify-counter-delivery strict
```

期望结果：

1. `counter-db-credentials` secret 校验通过，且不再指向本地 `file:` 路径。
2. `outbox-relay-worker` 与 `projector-worker` 的 dev overlay 继续消费 `counter-db-credentials`。
3. 两个 worker 的 Flux app 继续指向各自 dev overlay。
4. 两个 worker 的 dev overlay 继续显式保持 `replicas: 1`。

## 3. Promotion 前置条件

当前仓库对 counter delivery 的 promotion 覆盖 dev admission 和 staging 路径：

1. counter DB credentials secret 先通过 `just sops-verify-counter-db-credentials dev`。
2. 再通过 `just verify-counter-delivery strict`，确认 secret、overlay、Flux app、runbook 和 reference chain 没有漂移。
3. staging overlay 已创建，Flux app 指向正确路径，ENV 替换正确。

staging 部署还需要：
1. 创建 staging SOPS secrets（`counter-db-credentials-staging`）。
2. 加密 `infra/security/sops/staging/` 下的 secret 文件。

## 4. 漂移排查

当 `just verify-counter-delivery strict` 失败时，按以下顺序排查：

1. `cargo run -p repo-tools -- secrets verify-counter-db-credentials --env dev`
2. `infra/security/sops/dev/counter-db-credentials.enc.yaml`
3. `infra/k3s/overlays/dev/outbox-relay-worker/kustomization.yaml`
4. `infra/k3s/overlays/dev/projector-worker/kustomization.yaml`
5. `infra/k3s/overlays/staging/outbox-relay-worker/kustomization.yaml`
6. `infra/k3s/overlays/staging/projector-worker/kustomization.yaml`
7. `infra/gitops/flux/apps/outbox-relay-worker.yaml`
8. `infra/gitops/flux/apps/projector-worker.yaml`
9. `infra/gitops/flux/apps/staging-outbox-relay-worker.yaml`
10. `infra/gitops/flux/apps/staging-projector-worker.yaml`
11. `docs/operations/counter-service-reference-chain.md`
12. 本 runbook

## 5. 当前已知风险

1. `counter-service` 仍主要以内嵌库形式运行在 `web-bff` 内，不是独立 deployable 主路径。
2. staging SOPS secrets 尚未创建，staging overlay 不可直接 deploy。
3. prod overlay 尚未创建。
4. `commit_mutation` 使用 `execute_batch` + `BEGIN/COMMIT` 实现事务原子性。libsql 不支持 TEMPORARY table，CAS 结果通过事务后 `SELECT value, version FROM counter WHERE version = ?` 验证。如果事务与验证之间有并发写入，CAS 结果可能误判，但此时 CAS 冲突已由 SQLite 事务保证不会发生。

## 6. Rollback

当 counter delivery 在 dev admission 后出现问题时，按以下顺序回滚：

1. **Flux 暂停**：`flux suspend kustomization outbox-relay-worker projector-worker` — 停止 worker 拉取 external DB credentials。
2. **Overlay 回退**：恢复 `infra/k3s/overlays/dev/{outbox-relay,projector}-worker/kustomization.yaml` 到上一个已知正常版本。
3. **Secret 回退**：如果问题出在 counter DB credentials secret，回退 `infra/security/sops/dev/counter-db-credentials.enc.yaml`。
4. **验证**：重新执行 `just verify-counter-delivery strict`。
5. **Flux 恢复**：`flux resume kustomization outbox-relay-worker projector-worker`。

当前 rollback 仍是手动步骤，未自动化。未来应通过 `verify-counter-delivery` 的 failure 输出自动定位回滚靶点。
