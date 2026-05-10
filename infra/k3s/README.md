# K3s Delivery Layout

> 目的：说明 `infra/k3s/` 当前提供的是 K3s + Kustomize 交付落点，而不是“所有集群路径都已完成”的现状声明。

## 当前结构

```text
infra/k3s/
├── base/                   # 共享基础资源
├── overlays/
│   ├── dev/                # 当前最完整的单节点/开发环境 overlay
│   ├── staging/            # 目标中的预发 overlay，仍有明显占位成分
│   └── prod/               # 目标中的生产 overlay，仍需继续校准
└── scripts/                # 引导与部署脚本
```

## 当前事实

1. 当前交付方向是 `K3s + Flux + SOPS`。
2. `dev` overlay 是最真实的参考落点，已经显式挂接 shared counter DB secret。
3. `staging` 与 `prod` overlay 仍保留目标态成分，不能自动等价为“已验证可用”。
4. 这套目录表达的是交付形状与演进方向，不等于所有 deployable 都已独立完成。

## 入口

1. `base/`：共享 namespace、rbac、network policy、configmap、deployment、service、ingress。
2. `overlays/dev/`：当前默认参考 overlay。
3. `overlays/staging/` 与 `overlays/prod/`：环境化 patch 与资源约束占位。
4. `../../docs/operations/advanced-topology/gitops.md`：GitOps 与交付约束说明。

## 不要这样用

1. 不要把 README 写成“所有环境都已真实部署并验证”的状态说明。
2. 不要把 `prod` 目录中的 HPA/多副本配置误写成当前默认运行事实。
3. 不要绕过 SOPS/Flux 约束，把 `.env` 当成集群交付主路径。
