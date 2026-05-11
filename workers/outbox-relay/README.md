# Outbox Relay Worker

> 目的：说明 `outbox-relay` 作为默认异步发布链参考样例时，当前负责什么、入口在哪里，以及哪些部分仍未闭环。

## 状态

- status: `reference`
- 角色：默认 outbox -> relay -> publish 链路参考 worker
- 说明：当前 canonical relay path 是 `event_outbox -> outbox-relay`。它会把 canonical `contracts_events::EventEnvelope` 发布到当前消息骨干，并兼容把同一 envelope 转给 runtime pubsub；后者仍属于次级分发面，不应反向定义默认主链

## 责任

1. 轮询 outbox 并提取待发布事件。
2. 在发布前执行去重与幂等保护。
3. 维护 checkpoint、失败记录与恢复顺序。
4. 作为新增异步投递 worker 的默认结构参考。

## 可靠性契约

1. Delivery 语义：`at-least-once`。失败发布不会被声明为成功，重复事件由 idempotency/dedupe 处理。
2. Retry 语义：`event_outbox.retry_count < 5` 的 `pending`/`failed` 事件会被重新拉取；失败会写入 worker idempotency store，等待下一轮 outbox retry 策略处理。
3. Checkpoint 语义：checkpoint 只能在 entry 已经过发布处理路径后推进；它不是 exactly-once proof。
4. Dedupe 语义：以 outbox `event_id` 为重复处理键，防止 worker 重启或重复拉取时再次推进同一 entry。
5. Replica 语义：默认 reference profile 要保持 `replicas=1`。多副本安全需要 durable ownership、shared checkpoint、dedupe 与 recovery evidence 后才能提升。

## 入口

1. `src/main.rs`：主循环、health server、reader 选择逻辑。
2. `src/polling/`：outbox 读取与批处理。
3. `src/publish/`：事件发布逻辑。
4. `src/checkpoint/` 与 `src/idempotency/`：checkpoint、幂等与恢复状态。
5. `../../platform/model/deployables/outbox-relay-worker.yaml`：deployable 元数据入口。
6. `../../infra/k3s/overlays/dev/outbox-relay-worker/kustomization.yaml`：独立 dev overlay 入口。
7. `../../infra/gitops/flux/apps/outbox-relay-worker.yaml`：Flux GitOps 入口。

## 验证

```bash
cargo check -p outbox-relay-worker
cargo test -p outbox-relay-worker
just validate-resilience strict
```

## 不要这样用

1. 不要把当前 NATS 发布 adapter 写成“所有下游都已完成 broker 订阅”的最终形态。
2. 不要跳过 checkpoint、幂等、恢复顺序这些 worker 硬约束。
3. 不要把 `EventBus` 与 `runtime::PubSub` 的双写兼容面误写成“双 canonical 发布路径”。
4. 不要在 `counter-db-credentials` 仍指向本地 `file:` 路径，或 `just sops-verify-counter-db-credentials dev` / `just verify-counter-delivery strict` 仍未通过时，继续依赖 dev overlay 中已启用的 `outbox-relay-worker` 作为有效交付链路。
