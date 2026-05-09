# Local Development

> 目的：说明本仓库当前后端默认本地开发入口，以及它如何对齐 `counter-service` reference chain。
>
> 本文档不试图覆盖所有前端或桌面运行方式；默认视角仍然是后端主链。

## 1. 核心结论

当前本地开发应优先理解为两层：

1. 基础依赖层：`repo-tools infra local ...` 管理 NATS、Valkey、MinIO，以及可选的 sqld 相关基础设施。
2. 本地 auth 层：`repo-tools infra auth ...` 管理 `Generic OIDC + OpenFGA`，当前本地参考 IdP 是 Rauthy。
3. 应用运行层：通过 `just` / `moon` 启动 `web-bff`、其他 BFF 或需要的开发进程。

后端默认入口不是 `.env` 驱动的全仓库教程，而是围绕 `counter-service` 主链建立的最小本地闭环。

当前对 auth 的推荐理解也应分层：

1. `counter-service + tenant-service + web-bff` 是默认后端主链。
2. `Generic OIDC + OpenFGA` 是可选增强，不应成为所有本地后端开发的前提。
3. 若当前任务只关心后端 handler / service / contracts，可优先使用 `APP_AUTH_MODE=dev_headers` 做本地接口调试。

当前本地与 CI 的验证入口也按 profile 区分：

1. GitHub CI/CD 对齐的单机向基础门禁：`just gate-ci-single-node`
2. 低资源本地 Kubernetes profile：`just smoke-local-k3d` 或 `just gate-local-k3d`
3. 可选 auth lane：`just verify-auth-optional` + `just test-auth-optional`
4. 可选 SurrealDB lane：`just verify-backend-alternative` + `just test-backend-alternative`

## 2. 推荐阅读顺序

开始本地后端开发前，建议按以下顺序理解：

1. `docs/architecture/north-star.md`
2. `docs/operations/counter-service-reference-chain.md`
3. `infra/local/README.md`
4. `docs/operations/gate-profiles.md`
5. `docs/operations/k3d-local.md`
6. `justfiles/dev.just`
7. `justfiles/gates.just`
8. `justfiles/k3d.just`
9. `justfiles/sops.just`
10. `justfiles/ops.just`
11. `infra/docker/compose/core.yaml`

## 2.1 平台前置条件

当前仓库的本地后端主链并不是“所有命令在三平台纯原生完全等价”。更准确的理解是：

1. macOS / Linux：默认支持 `just` / `moon` / `cargo` 与本地容器 runtime。
2. Windows：Rust / Bun / Node / just / moon 本身可以原生运行；`repo-tools infra local ...` 不要求 Git Bash/WSL，但仍需要 Docker Desktop 或 Podman Desktop。
3. Linux host 专属操作，例如 k3s bootstrap apply、VPS bootstrap apply、systemd deploy，不是 Windows 桌面命令。

当前已经确认的现实约束：

1. `just dev-api`、`just check-backend-primary`、`just test-backend-primary` 这类 `cargo` / `moon` 主链命令更接近跨平台。
2. `repo-tools secrets ...`、`repo-tools infra local ...`、`repo-tools ops migrate ...` 是 Rust 控制面入口；它们的跨平台能力仍取决于外部工具是否可用。
3. `repo-tools infra auth ...` 命令层不再依赖本地 shell 脚本，但当前仍使用 Podman 运行 auth compose stack。
4. 这意味着 Windows 支持应按命令声明，而不是笼统承诺所有 infra 操作都可在 Windows 桌面原生执行。

## 3. 当前真实入口

### 3.1 工具链准备

使用仓库已有命令：

```bash
just setup
just doctor
just doctor-full
```

这些命令比手写安装步骤更接近当前仓库的真实入口。

可选 app shell 依赖从 app 自己的作用域安装，例如 `bun install --cwd apps/web`。

### 3.2 启动基础依赖

当前本地基础设施入口是：

```bash
just deploy-dev
just status-dev
```

当任务涉及本地 `Generic OIDC/OpenFGA` 时，再额外启动：

```bash
just auth-bootstrap
# load infra/local/generated/auth.env in your shell if the process needs those values
```

脚本当前会管理的核心依赖包括：

1. NATS
2. Valkey
3. MinIO
4. 可选的 Turso/libSQL client-server 模式相关端口信息
5. 可选的本地 auth 栈：`http://localhost:8082/auth/v1/` (Rauthy local reference IdP), `http://localhost:8081` (OpenFGA)
6. 可选的本地 SurrealDB server：`http://localhost:8000`

需要注意：

1. 默认业务路径仍主要使用嵌入式 libSQL/SQLite 形态。
2. sqld 是可选的本地实验路径，不应写成所有开发都必须依赖的默认前提。
3. SurrealDB 是可选实验 lane，默认按独立 DB server 使用，不应让默认主链编译 SurrealDB Rust SDK。

### 3.3 启动后端开发进程

当前仓库真实存在的 just 入口包括：

```bash
just dev
just dev-api
just auth-bootstrap
just auth-up
just auth-down
```

其中：

1. `just dev-api` 是更贴近后端默认视角的入口之一。
2. root backend-core contract 不再暴露前端或桌面壳层入口。
3. `just auth-bootstrap` 会把本地 `Rauthy/OpenFGA` 起起来，并生成 generic `APP_OIDC_*` / `APP_AUTHZ_*` 到 `infra/local/generated/auth.env` 供 `web-bff` 直接读取。
4. `just check-backend-primary` / `just test-backend-primary` 对应默认后端 admission lane。
5. `just verify-auth-optional` / `just test-auth-optional` 仅在 auth lane 变更时需要额外运行。

补充约束：

1. 如果你的任务不涉及 `apps/desktop/**`，不要把 Tauri 当成必须前置条件。
2. 如果你的任务涉及桌面壳层，请在对应 shell 自己的目录和命令面上验证，不要把这些要求带回 root backend-core contract。
3. 不要假设 Ubuntu CI 能替代 macOS / Windows 桌面行为。

### 3.3.1 本地存储和缓存维护

Template 使用者会频繁升级依赖和镜像。默认清理入口必须安全、可重复、不会意外删除业务状态：

```bash
just clean-local-storage
```

该入口只做保守维护：

1. 截断 `.tmp` 中过大的测试日志。
2. 清理当前 Cargo workspace 不再使用的依赖构建缓存。
3. 清理 7 天未访问的 Cargo build artifacts。

不会自动删除：

1. Compose volumes，例如 MinIO、Valkey、NATS、OpenFGA 本地状态。
2. 全局 mise、Bun、Node、Cargo registry 缓存。
3. SOPS、age、Kubernetes 或 GitOps 相关本地状态。

如果确实要删除本地 compose volumes，必须显式使用对应 infra 命令和 destructive flag，例如：

```bash
cargo run -p repo-tools -- infra local down --volumes
```

这条原则和版本升级策略一致：先 pin 和 smoke，再清理过期缓存；不要用大范围删除来掩盖版本或迁移问题。

### 3.3.2 后端优先的最小调试模式

如果当前任务只围绕后端接口、tenant flow、counter flow，而不希望被 `web` / `tauri` / 本地 OIDC provider 阻塞，可以直接使用：

```bash
export APP_DATABASE_URL=file:./.data/web-bff.db
export APP_AUTH_MODE=dev_headers
cargo run -p web-bff
```

此时受保护 API 可以通过本地开发头注入身份：

```bash
curl -X POST http://localhost:3010/api/tenant/init \
  -H 'content-type: application/json' \
  -H 'x-dev-user-sub: local-dev-user' \
  -d '{"user_sub":"local-dev-user","user_name":"Local Dev"}'

curl http://localhost:3010/api/counter/value \
  -H 'x-dev-user-sub: local-dev-user'
```

可选开发头：

1. `x-dev-user-sub`：必填，本地用户标识。
2. `x-dev-tenant-id`：可选，若已知 tenant id 可显式传入；通常不需要，`tenant/init` 后会从数据库绑定解析。
3. `x-dev-user-roles`：可选，逗号分隔角色，仅用于本地调试上下文。

这个模式的目标不是替代真实 auth，而是降低后端开发、接口测试、issue 复现的成本。

当前 `web-bff` 受保护接口的本地错误矩阵也应按统一契约理解：

1. `401 Unauthorized`：缺少 bearer token、token 无效，或缺少 authenticated request context。
2. `403 Forbidden`：tenant claim 与持久化 tenant binding 不一致，或 authz check 明确拒绝。
3. `404 NotFound`：`GET /api/user/me` 的当前用户尚无 profile 记录。
4. `415 BadRequest(code) + HTTP 415`：`POST /api/tenant/init` 未提供 `application/json`。
5. `422 ValidationError`：`tenant/init` 请求体字段缺失或校验失败。

### 3.4 本地 secrets 注入

当前后端参考路径不应把 `.env` 当成主路径。更符合当前仓库约束的 SOPS 对齐方式是：

```bash
just sops-run web-bff dev
just sops-run outbox-relay-worker dev 'cargo run -p outbox-relay-worker'
just sops-run projector-worker dev 'cargo run -p projector-worker'
```

原因：

1. 这与集群中的 `SOPS -> Kustomize/Flux` 路径保持环境变量形状一致。
2. 可以避免本地路径和交付路径分叉得过早。
3. 若要走本地 Podman auth 栈，可先 `source infra/local/generated/auth.env`，再用 host-process 或 `just sops-run` 启动 `web-bff`。

## 4. 平台支持边界

| 命令或脚本 | 平台承诺 |
| --- | --- |
| `repo-tools secrets ...` | 跨平台控制面；需要 `sops` / `age` 等外部工具。 |
| `repo-tools infra local ...` | 跨平台控制面；需要 Docker Desktop 或 Podman Desktop。 |
| `repo-tools infra auth ...` | 命令层跨平台；当前 auth stack 仍以 Podman compose 为主要 runtime。 |
| `repo-tools infra k3s deploy ...` | 可从开发机发起；需要 `kubectl` / `kustomize` 与集群访问。 |
| `repo-tools ops migrate ...` | 跨平台控制面；`--apply` 需要本机可用的 `sqlite3`。 |
| `infra/docker/docker-entrypoint-gateway.sh` | 容器镜像 entrypoint，不是宿主机控制面命令。 |
| `infra/k3s/scripts/bootstrap-k3s.sh` | Linux host only；不是 Windows 桌面命令。 |
| `ops/scripts/bootstrap/vps.sh` | Linux VPS host only；不是 Windows 桌面命令。 |
| systemd recipes | Linux/systemd only。 |

## 5. 以 counter 参考链理解本地开发

当前本地后端最值得优先跑通的不是“所有模块一起启动”，而是下面这条最小主线：

1. 启动本地基础依赖。
2. 启动 `web-bff` 或对应后端入口。
3. 必要时启动 `outbox-relay-worker`。
4. 需要验证 read model/replay 时，再启动 `projector-worker`。
5. 观察 counter 相关同步写入、outbox 写入和异步发布路径。

这样做的意义是：

1. 优先验证默认后端锚点。
2. 优先对齐后续服务应复用的工程路径。
3. 避免把还未收敛完成的外围模块当成默认学习入口。
