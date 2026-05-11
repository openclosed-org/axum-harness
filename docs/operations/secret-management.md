# Secret Management

> 目的：说明本仓库后端默认如何管理 secrets，以及它们如何挂接到 `counter-service` reference chain。
>
> 这不是通用 SOPS 教程；它只描述当前仓库里真实存在的路径、脚本和约束。

## 1. 核心结论

当前后端 canonical secret contract 是 SOPS/age-backed deployable environment shape：

1. 可提交的 secret shape example 放在 `infra/security/sops/templates/<env>/*.example.yaml`
2. 本机明文 secret 输入放在同目录 `*.yaml`，但必须被 git 忽略
3. 加密产物放在 `infra/security/sops/<env>/*.enc.yaml`
4. 加密规则由根目录 `.sops.yaml` 统一定义
5. 本地非集群运行时，可通过 `just sops-run` 将解密后的环境变量注入进程
6. 单 VPS `systemd-binary` 或 `podman` profile 可通过 `just sops-export-env` 在宿主机生成临时 `0600` env-file
7. 集群路径通过 Kustomize/Flux 消费加密 secrets，而不是依赖 `.env`

这条路径是 `counter-service` 工程横切链的一部分，不是旁路能力。短本地调试可以使用显式 `APP_*` exports，但不能把 `.env` 或导出的临时 env-file 提交为后端参考配置源。

## 2. 当前真实文件落点

### 2.1 SOPS 规则入口

主要文件：

1. `.sops.yaml`
2. `justfiles/ops/sops.just`

当前已确认的事实：

1. `.sops.yaml` 已定义本机 plaintext secret 输入、`dev/`、`staging/`、`prod/` 的创建规则。
2. `justfiles/ops/sops.just` 已把仓库 secret shape 写成 `SOPS + age`，并明确说明后端参考路径不来自 `.env`。
3. 当前建议的命令入口是 `just sops-gen-age-key`、`just sops-edit`、`just sops-encrypt-dev`、`just sops-run`、`just sops-export-env`、`just sops-reconcile`。

### 2.2 与 counter 参考链直接相关的模板

主要文件：

1. `infra/security/sops/templates/dev/web-bff.example.yaml`
2. `infra/security/sops/templates/dev/counter-db-credentials.example.yaml`
3. 本机可从 example 复制出 ignored plaintext `*.yaml` 后再加密

对应的加密产物当前也已存在：

1. `infra/security/sops/dev/web-bff.enc.yaml`
2. `infra/security/sops/dev/outbox-relay-worker.enc.yaml`
3. `infra/security/sops/dev/projector-worker.enc.yaml`
4. `infra/security/sops/dev/counter-db-credentials.enc.yaml`
5. `infra/security/sops/dev/counter-service.enc.yaml`
6. `infra/security/sops/dev/surrealdb.enc.yaml`

需要注意：

1. `web-bff` secrets 对应当前 counter 的同步主路径。
2. `outbox-relay-worker` secrets 对应当前异步 relay 主路径。
3. `projector-worker` secrets 已有 dev 模板与加密产物，当前 overlay 已显式配置 `replicas=1`，因此 counter DB credentials 校验必须前置到 admission。
4. `counter-db-credentials` 是 database capability `external_single_node` 或 `external_distributed` 的 secret source，用来把 `web-bff`、`outbox-relay-worker`、`projector-worker` 指向同一份远程 libSQL/Turso 数据源。
5. `counter-service` secrets 已有模板和加密产物，但模板本身已明确说明它主要为 Phase 1+ 独立 deployable 预留。
6. `surrealdb` secrets 支撑可选 SurrealDB provider lane 和 K3d/compose runtime profile，不是默认 backend-core 的必需前置。

因此，当前默认理解应是：

1. counter 的 secrets 链路已经有真实落点。
2. 但独立 `counter-service` deployable 仍不是当前主运行形态。

## 3. 默认操作路径

### 3.1 首次设置 age key

使用仓库已有命令：

```bash
just sops-gen-age-key
just sops-show-age-key
```

然后更新根目录 `.sops.yaml` 中对应环境的 public key。

当前建议立刻再跑一次：

```bash
just sops-validate
```

这条命令现在应同时回答四件事：

1. `.sops.yaml` 是否存在。
2. `~/.config/sops/age/key.txt` 是否存在。
3. 当前 age public key 是否真的出现在 `.sops.yaml` 的 `creation_rules` 中。
4. 当前 key 是否真的能解开至少一个 `infra/security/sops/dev/*.enc.yaml` 文件。

如果这里失败，优先按下面顺序判断：

1. 没有 key：先执行 `just sops-gen-age-key`。
2. key 存在，但 `.sops.yaml` 里没有对应 public key：执行 `just sops-show-age-key`，然后把 public key 写入 `.sops.yaml`。
3. key 和 `.sops.yaml` 看起来一致，但仍无法解密：说明当前 `.enc.yaml` 很可能不是用这把 key 加密的，需要重新加密或切换到正确私钥。

### 3.2 编辑或生成某个 deployable 的 secrets

推荐命令：

```bash
just sops-edit web-bff dev
just sops-edit outbox-relay-worker dev
just sops-edit projector-worker dev
just sops-encrypt-dev web-bff
```

当前更符合仓库结构的做法是：

1. 从 `infra/security/sops/templates/<env>/<deployable>.example.yaml` 复制出本机 ignored plaintext `infra/security/sops/templates/<env>/<deployable>.yaml`
2. 在本机 plaintext 文件中填入真实值
3. 重新加密生成 `infra/security/sops/<env>/<deployable>.enc.yaml`
4. 提交加密产物和 sanitized `.example.yaml` shape，不提交明文 secret 或 `.env`

### 3.3 本地非集群运行

本地后端参考路径不要通过 `.env` 注入 secrets。当前仓库提供的 SOPS 对齐内环路径是：

```bash
just sops-run web-bff dev
just sops-run outbox-relay-worker dev 'cargo run -p outbox-relay-worker'
just sops-run projector-worker dev 'cargo run -p projector-worker'
just sops-verify-counter-db-credentials dev
```

这条路径的意义是：

1. 让本地进程消费和集群一致的环境变量形状。
2. 避免为了开发临时制造新的 `.env` 主路径。
3. `sops-run` 默认数据库能力状态是 `local_real`，即本地 durable embedded libSQL/SQLite，不合并 `counter-db-credentials`。
4. 需要 external DB 时必须显式选择 `external_single_node` 或 `external_distributed`。
5. 在继续依赖当前已启用的独立 worker overlay 前，先验证 external DB secret 不再指向本地 `file:` 路径。

数据库能力状态必须使用统一五态词汇：

| state | 当前 `sops-run` 语义 |
|---|---|
| `disabled` | 不支持；当前 backend reference chain 需要 durable DB，命令会拒绝。 |
| `local_mock` | 不支持；DB lane 不提供 mock fallback，命令会拒绝。 |
| `local_real` | 默认；只注入 deployable secret，使用本地 durable embedded DB 配置。 |
| `external_single_node` | 显式合并 `counter-db-credentials.enc.yaml`，用于 Turso/libSQL remote 或单节点外部 DB。 |
| `external_distributed` | 显式合并 `counter-db-credentials.enc.yaml`，用于集群/托管分布式 DB 语义。 |

推荐的快速验证链：

```bash
just sops-validate
just sops-verify-counter-db-credentials dev
cargo run -p repo-tools -- secrets decrypt-env infra/security/sops/dev/web-bff.enc.yaml
just sops-run web-bff dev 'cargo run -p web-bff' local_real
just sops-run web-bff dev 'cargo run -p web-bff' external_single_node
```

其中：

1. `just sops-validate` 验证 key 与 `.sops.yaml`、以及样例解密是否真实可用。
2. `just sops-verify-counter-db-credentials dev` 验证 external DB secret 是否仍残留模板占位符，是否错误指向本地 `file:` URL。
3. `repo-tools secrets decrypt-env` 可直接观察当前会注入哪些环境变量。
4. `just sops-run` 则是最终运行态验证。

当前常见失败信号及含义：

1. `Age key not found`：本机还没有 `~/.config/sops/age/key.txt`。
2. `Age public key is not present in .sops.yaml`：本机私钥对应的 public key 没被加入仓库 SOPS 规则。
3. `failed to decrypt`：仓库内的 `.enc.yaml` 不是用当前私钥加密，或 `SOPS_AGE_KEY_FILE` 指向了错误文件。
4. `REPLACE_WITH_TURSO_TOKEN`：模板占位符还没被真实 secret 替换，不应继续把这份 secret 当成可运行配置。

### 3.4 单 VPS `systemd-binary` 与 `podman` profile

单 VPS profile 不使用 `.env`。它们和 `sops-run` 使用同一份 `infra/security/sops/<env>/*.enc.yaml`，区别是注入边界从当前子进程变为宿主机上的临时 env-file：

```bash
just sops-export-env web-bff dev systemd-binary /run/axum-harness/web-bff.env
just sops-export-env web-bff dev podman /run/axum-harness/web-bff.env
```

当前已检查的行为：

1. 导出命令面向单 VPS external DB capability state，会合并 `<deployable>.enc.yaml` 与 `counter-db-credentials.enc.yaml`。
2. 输出文件以 `0600` 权限写入。
3. 缺失 deployable 专属 secret 时会失败，避免只拿 shared DB secret 误启动未知服务。
4. 输出路径应位于 `/run/...`、`/var/run/...`、受控 secret 目录，或本地测试用 `.run/...`；不要提交。
5. macOS 本机已用导出的 env-file 启动 `web-bff` host process，并通过 `GET /healthz`。
6. macOS Podman 已用导出的 env-file 启动 `web-bff` 容器，并通过宿主机 `GET /healthz`。

`systemd-binary` 使用方式：

```ini
[Service]
EnvironmentFile=/run/axum-harness/web-bff.env
ExecStart=/opt/axum-harness/bin/web-bff
```

`podman` 使用方式：

```bash
podman run --rm --env-file /run/axum-harness/web-bff.env <image>
```

因此默认不需要把 `sops` 或 `age` 装进应用容器。解密发生在宿主机控制面；容器只接收普通环境变量。如果后续使用 Quadlet，也应指向同一个宿主机 env-file，而不是在镜像启动脚本里解密 secret。

macOS smoke 注意事项：

1. Podman machine 需要足够内存构建 `web-bff` 镜像；2GiB 已观测到会在 release build 中被 OOM kill。
2. `web-bff` 镜像当前使用 `rust:1.95-bookworm` builder 和 `gcr.io/distroless/cc-debian12:nonroot` runtime，避免 glibc 版本漂移与动态链接 loader 缺失。
3. 本机 smoke 可覆盖 `APP_DATABASE_URL=file:/tmp/web-bff.db`，并避免使用远程 Turso secret；这证明注入与启动路径，不证明远程数据库可用性。

## 4. 与 Kustomize / Flux 的关系

secrets 文档不能脱离部署链路单独理解。当前真实挂接关系是：

1. `infra/k3s/overlays/dev/kustomization.yaml` 已引用：
   - `web-bff.enc.yaml`
   - `outbox-relay-worker.enc.yaml`
   - `counter-db-credentials/kustomization.yaml`
2. `infra/k3s/overlays/dev/projector-worker/kustomization.yaml` 与 `infra/k3s/overlays/dev/outbox-relay-worker/kustomization.yaml` 都已显式挂接 `counter-db-credentials` secret，并在当前清单中将副本数显式配置为 1。
3. 同文件中 `counter-service.enc.yaml` 当前仍被注释，注释明确说明其对应未来独立 deployable 阶段。
4. `infra/gitops/flux/apps/*.yaml` 已声明通过 `decryption.provider: sops` 和 `secretRef.name: sops-age` 解密。
5. 本地 K3d smoke 不走 Flux；它通过 `just sops-reconcile dev` 直接 apply SOPS 解密后的 Secret，并创建 `app` 与 `app-dev` namespace。
6. `surrealdb.enc.yaml` 当前落到 `app` namespace；已有服务/worker dev secrets 主要落到 `app-dev` namespace。

因此这条链路当前的正确理解是：

1. secrets shape 已进入默认工程主线。
2. 但 `counter-service` 本体仍主要通过 `web-bff` 承载，而不是通过独立 deployable 完整消费自身 secrets。
3. SurrealDB secret shape 已进入可选 provider/runtime profile；不能把它写成默认 backend-core 本地开发必需项。

## 5. 文档边界

这份文档只回答以下问题：

1. secrets 存在哪里。
2. 如何编辑与加密。
3. 如何挂接到本地进程和集群路径。
4. 它和 `counter-service` reference chain 的关系是什么。

这份文档不负责：

1. 讲解通用 SOPS/age 全部知识。
2. 保证当前 Flux/Kustomize 清单已经完全闭环。
3. 将尚未实现的独立 `counter-service` deployable 写成既成事实。

## 6. 一句话结论

当前后端 canonical secret shape 已经是 `*.example.yaml -> ignored local *.yaml -> enc.yaml -> sops-run / sops-export-env / Kustomize-Flux`。DB/storage lane 必须用 `disabled|local_mock|local_real|external_single_node|external_distributed` 表达状态，不能用 `shared-db` 这类实现昵称替代 capability state。
