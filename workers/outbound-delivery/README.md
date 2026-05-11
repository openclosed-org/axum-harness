# Outbound Delivery Worker

## 状态

- status: `reference-slice`
- 角色：notification/outbound webhook outbox delivery worker 的 Phase 3 最小样例。
- 当前实现：进程入口、health/readiness、noop transport composition。真实 provider transport 和 durable DB adapter 仍未接入默认拓扑。

## 责任

1. 从 delivery outbox 拉取 due jobs。
2. 为 outbound webhook delivery 生成签名 header。
3. 对失败 delivery 执行 retry policy。
4. 将耗尽 retry budget 的 delivery 标记为 dead-letter。

## 可靠性契约

1. Delivery 语义：`at-least-once`。delivery transport 成功后才标记 delivered。
2. Retry 语义：`RetryPolicy` 决定最大尝试次数和 backoff；失败不会被声明成功。
3. Checkpoint/replay 语义：当前 reference slice 使用 delivery outbox 状态作为恢复游标；durable DB adapter 接入前不声明跨进程 replay proof。
4. Idempotency 语义：每个 `DeliveryJob` 有稳定 id；provider adapter 必须把 job id 或 event id 传入幂等 key/header。
5. Dead-letter 语义：超过 retry budget 的 job 进入 `DeadLettered` 状态，等待人工或 replay 工具处理。
6. Recovery 语义：当前 worker 可安全重启，但默认 noop/in-memory composition 只证明语义，不证明 durable recovery。

## 验证

```bash
cargo test -p notification
cargo check -p outbound-delivery-worker
```

## 不要这样用

1. 不要从 service/domain code 直接调用邮件、短信或 webhook provider SDK。
2. 不要把 Valkey/cache 当作 notification/webhook delivery source of truth。
3. 不要在没有签名、retry、dead-letter 语义时启用 public outbound webhook。
4. 不要把当前 noop transport 写成真实 provider delivery evidence。
