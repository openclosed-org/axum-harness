# Base K3s Manifests

> 共享基础资源层。这里描述当前实际存在的基础清单，不额外推断所有组件都已完成生产验证。

## 当前资源

| File | Purpose |
|------|---------|
| `namespace.yaml` | 应用 namespace |
| `rbac.yaml` | 基础权限资源 |
| `network-policy.yaml` | 基础网络策略 |
| `configmap.yaml` | 通用配置 |
| `configmap-outbox-relay-worker.yaml` | outbox relay worker 配置 |
| `configmap-projector-worker.yaml` | projector worker 配置 |
| `deployment-web-bff.yaml` | `web-bff` 部署 |
| `deployment-outbox-relay-worker.yaml` | outbox relay worker 部署 |
| `deployment-projector-worker.yaml` | projector worker 部署 |
| `service.yaml` | ClusterIP services |
| `ingress.yaml` | 基础 ingress 规则 |
| `kustomization.yaml` | Kustomize 入口 |

## 当前边界

1. 这里的 base 主要覆盖默认参考链相关 deployables。
2. 没有出现在这里的 deployable，不能因为平台模型中存在就被视为已接入 base。
3. 事件与 worker 交付链虽然已有清单，但其可靠性语义仍要以代码与 docs 中的当前状态说明为准。

## 使用方式

优先通过 overlay 应用，而不是直接把 base 当环境清单：

```bash
kubectl apply -k infra/k3s/overlays/dev
```
