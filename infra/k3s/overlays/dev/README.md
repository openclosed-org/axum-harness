# Dev Overlay

> 当前最真实的 K3s 参考 overlay。用于单节点、开发态、默认参考链验证。

## 当前事实

1. 显式引用 `counter-db-credentials` secret kustomization。
2. 已包含 `web-bff`、`outbox-relay-worker` 等默认参考链所需 secret/patch。
3. 仍以单节点、低副本和开发资源限制为主，不等于生产验证完成。

## 使用

```bash
kubectl apply -k infra/k3s/overlays/dev
```
