# Platform Model README

> 目标：把平台级声明集中成可验证的 metadata index，同时避免把每个 service 的细粒度业务语义错误地塞进平台层。

---

## 1. 定位

`platform/model/*` 是平台控制面的声明索引。它比下列派生或承载文件更适合表达平台意图，但仍需要 validators、drift checks、scripts、gates 或运行证据支撑更强结论：

1. `infra/*` 的承载细节
2. `docs/generated/*` 的生成产物
3. `packages/sdk/*` 的生成代码
4. `servers/*` 与 `workers/*` 的实现骨架

它回答的是：

1. 系统有哪些平台级 service 元数据？
2. 哪些 deployable 存在？它们是 stateless 还是 checkpointed / stateful？
3. 哪些 workflow 是平台级长事务？
4. 哪些 cross-cutting capability 存在、支持哪些五态、有哪些 provider/adapter/resource entity？
5. 哪些 topology 被支持？
6. 有哪些全局 owner / consistency / idempotency 默认规则？
7. failure domain 与基础 SLO 基线是什么？

它不负责：

1. 每个 service 的细粒度 owns_entities / commands / events / queries 声明
2. 具体业务实现代码
3. 临时运维脚本
4. 手工维护的 rendered manifests

---

## 2. 关键边界

### 2.1 平台层 vs Service 层

这是重建后的关键边界：

| 层 | 归属 | 负责什么 |
|---|---|---|
| `platform/model/*` | `platform-ops-agent` | 平台级元数据、全局规则、deployable、workflow、topology、resource、environment |
| `services/<name>/model.yaml` | `service-agent` | service-local distributed semantics：owns_entities、commands、events、queries、consistency、cross-service reads |

如果一个文件主要回答“这个 service 拥有什么状态、如何对外读写、有什么一致性和幂等要求”，它应当在 `services/<name>/model.yaml`，而不是 `platform/model/state/**`。

### 2.2 为什么不把 service 语义拆散到 `platform/model/state/**`

原因有三：

1. 会制造过多零散 YAML，增加导航成本。
2. 会让 `platform-ops-agent` 被迫维护它并不拥有的业务语义。
3. 会和 `service-agent` 的写边界冲突。

因此：

1. `platform/model/state/ownership-map.yaml` 只保留全局 entity → owner 映射。
2. `platform/model/state/consistency-defaults.yaml` 只保留全局默认一致性规则。
3. `platform/model/state/idempotency-defaults.yaml` 只保留全局默认幂等规则。

---

## 3. 目录职责

```text
platform/model/
├── services/                  # 平台级 service 元数据
├── deployables/               # 可部署单元定义
├── resources/                 # 外部资源定义
├── capabilities/              # 横切能力五态、provider、adapter、resource entity 声明
├── workflows/                 # 长事务 / durable workflow 定义
├── policies/                  # 平台策略定义
├── topologies/                # 承载组合定义
├── environments/              # 环境差异抽象
├── state/
│   ├── ownership-map.yaml     # 全局 entity → owner 映射
│   ├── consistency-defaults.yaml
│   └── idempotency-defaults.yaml
├── partitioning/
│   └── defaults.yaml
├── failures/
│   └── domains.yaml
└── slo/
    └── defaults.yaml
```

### 3.1 `services/`

只定义平台级 service 元数据，例如：

1. `name`
2. `kind`
3. `criticality`
4. `tenant_scope`
5. `logical_dependencies`
6. `status`

### 3.2 `deployables/`

至少声明：

1. `kind`
2. `hosts_services`
3. `runtime_profile`
4. `statefulness`
5. `required_identity`
6. `required_storage`
7. `resource_bindings`
8. `scaling_axis`
9. `failure_domain`

### 3.3 `workflows/`

至少声明：

1. `trigger`
2. `idempotency_key`
3. `timeout`
4. `checkpoint_policy`
5. `steps`
6. `compensation`
7. `recovery`

### 3.4 `state/`

只保留平台级全局规则，不存放每个 service 的局部语义文件。

### 3.5 `capabilities/`

Capability model 使用 ECS 命名规则：entity name 保持稳定，state/provider/adapter/resource binding 分开表达。正式规则见 `docs/architecture/capability-naming.md`。

至少声明：

1. `key`：能力键，例如 `database`。
2. `states`：固定五态 `disabled|local_mock|local_real|external_single_node|external_distributed`。
3. `providers`：必须用 `<capability>.<provider>` 形式，例如 `database.turso-cloud`。
4. `adapters`：必须用 `<capability>.<adapter>` 形式。
5. `resource_entities`：稳定逻辑资源名，例如 `counter-db`，不能编码 `shared`、`local`、`external` 等状态/拓扑词。

---

## 4. 与 Reference Modules 的关系

参考样例固定为：

1. `counter-service`：最小完整链路、CAS、event、projection 样例
2. `tenant-service`：多租户、多实体、workflow、补偿样例
3. 如需继续扩大参考集，应在 `counter-service`、`tenant-service` 两个样例稳定后再追加。

这些样例的细粒度语义全部在各自 `services/<name>/model.yaml` 中表达。
平台层只保留它们的元数据与全局约束。

---

## 5. 变化顺序

当新增或修改能力时，推荐顺序：

1. 先扩展 `platform/schema/*`（若当前 schema 无法表达）
2. 再修改 `platform/model/*` 的平台级元数据或全局规则
3. 再修改 `services/<name>/model.yaml`
4. 再修改 `packages/contracts/*`
5. 再写 `services/*/src/**`
6. 最后改 `servers/*`、`workers/*`、`infra/*`

---

## 6. 一句话原则

> 平台层负责平台级声明索引，service 层负责 service 自己的分布式语义声明；
> 不让 `platform-ops-agent` 背业务语义，也不让 `service-agent` 越权进入平台控制面。
