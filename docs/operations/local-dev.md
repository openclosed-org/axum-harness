# Local Development

> 目的：说明本仓库当前后端默认本地开发入口，以及它如何对齐 `counter-service` reference chain。
>
> 本文档不试图覆盖所有前端或桌面运行方式；默认视角仍然是后端主链。

## 1. 核心结论

当前本地开发应优先理解为两层：

1. 资源容器层：`just podman-resources-*` 按需管理官方资源容器，例如 SurrealDB、NATS、Valkey、MinIO，以及可选 sqld/libSQL server；`repo-tools infra local ...` 是较底层的 compose 控制面。
2. 本地 auth 层：`repo-tools infra auth ...` 管理 `Generic OIDC + OpenFGA`，当前本地参考 IdP 是 Rauthy。
3. 应用运行层：通过 `just` / `moon` 启动 `web-bff`、其他 BFF 或需要的开发进程。

后端默认入口不是 `.env` 驱动的全仓库教程，而是围绕 `counter-service` 主链建立的最小本地闭环。

当前对 auth 的推荐理解也应分层：

1. `counter-service + tenant-service + web-bff` 是默认后端主链。
2. `Generic OIDC + OpenFGA` 是可选增强，不应成为所有本地后端开发的前提。
3. 若当前任务只关心后端 handler / service / contracts，可优先使用 `APP_AUTH_MODE=dev_headers` 做本地接口调试。

当前本地与 CI 的验证入口也按 profile 区分。默认路径优先保持低资源，重组件是 opt-in：

1. GitHub CI/CD 对齐的单机向基础门禁：`just gate-ci-single-node`
2. 低资源本地 Kubernetes profile：`just smoke-local-k3d` 或 `just gate-local-k3d`
3. 可选 auth lane：`just verify-auth-optional` + `just test-auth-optional`
4. 可选 SurrealDB lane：`just verify-backend-alternative` + `just test-backend-alternative`

单 VPS 和本地开发的资源预算按 preset 理解：

1. `lite`：默认低资源形态，使用 embedded libSQL/SQLite、Turso Cloud（可选托管）、Moka、stdout/journald、本地文件或禁用的轻量 adapter。
2. `standard`：按需开启 Podman 管理的 SurrealDB、NATS、Valkey 等官方资源容器。
3. `full`：完整单机资源栈，用于业务压力证明需要之后，不是最小启动门槛。

## 2. 推荐阅读顺序

开始本地后端开发前，建议按以下顺序理解：

1. `docs/architecture/north-star.md`
2. `docs/operations/counter-service-reference-chain.md`
3. `infra/local/README.md`
4. `docs/operations/gate-profiles.md`
5. `docs/operations/advanced-topology/k3d-local.md`
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

当前本地轻量资源入口是：

```bash
just deploy-dev
just status-dev
```

`just deploy-dev` 对齐 `lite` preset，不启动重资源容器。如果需要官方资源容器，显式选择：

```bash
just podman-resources-up surrealdb
just podman-resources-up standard
just podman-resources-up full
```

当任务涉及本地 `Generic OIDC/OpenFGA` 时，再额外启动：

```bash
just auth-bootstrap
# load infra/local/generated/auth.env in your shell if the process needs those values
```

脚本可按需管理的核心依赖包括：

1. NATS
2. Valkey
3. MinIO
4. 可选的 Turso/libSQL client-server 模式相关端口信息
5. 可选的本地 auth 栈：`http://localhost:8082/auth/v1/` (Rauthy local reference IdP), `http://localhost:8081` (OpenFGA)
6. 可选的本地 SurrealDB server：`http://localhost:8000`

需要注意：

1. 默认业务路径仍主要使用嵌入式 libSQL/SQLite 形态。
2. Turso Cloud 是可选托管 backend，可用于低资源 VPS 外部化数据库而不运行本地 DB 容器。
3. sqld 是可选的本地实验路径，不应写成所有开发都必须依赖的默认前提。
4. SurrealDB 是本项目优先考虑的可选外部 DB server lane；启用时按独立官方容器/进程管理，不应让默认主链编译 SurrealDB Rust SDK。
5. PostgreSQL 不是当前仓库 reference backend。

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

### 3.3.2 Rust 与 Podman 磁盘预期

Rust 后端模板的运行镜像可以很小，但开发期编译缓存不会小。新用户应先建立这个预期：

1. Rust release binary/runtime artifact 可以很小，但 `target/` 不会小。
2. Rust `target/` 在活跃开发中可能增长到 `20-50GB`。
3. 多次 `test`、`clippy`、release build、失败的 image build 后，`target/` 加容器缓存合计达到 `50-100GB` 并不异常。
4. Cargo registry/git cache 通常另占数 GB。
5. 如果把 first-party Rust app 放到 Podman 内编译，builder base image 和 dangling build layers 会占数 GB；这不是 single-VPS 推荐路径。

建议硬件预期：

1. 最低可尝试：4 cores、8GiB RAM、30GiB free disk。
2. 推荐本地开发：8 cores、16GiB RAM、60GiB+ free disk。
3. single-VPS runtime host 不应承担 Rust 编译。VPS 只接收本地/CI 产出的 binary 或 prebuilt image。
4. Podman machine disk 推荐 `100GiB` 只适用于本地反复运行资源容器、镜像验证或可选 image proof，不是低配 VPS 的默认要求。

常用观测命令：

```bash
just storage-report
just sccache-stats
just podman-doctor
just podman-disk
```

常用保守清理命令：

```bash
just clean-local-storage
just podman-prune-build-cache
just clean-run-artifacts
```

`just podman-prune-build-cache` 只清理 dangling images/build layers，不删除 volumes。若要更激进清理 unused containers/images/networks，可用：

```bash
just podman-prune-unused
```

不要默认使用 volume cleanup；MinIO、Valkey、NATS、OpenFGA、SurrealDB 等本地状态可能保存在 volumes 中。

若明确要释放所有本地编译产物、sccache、Podman images/containers/volumes，可使用显式破坏性命令：

```bash
just clean-aggressive-local
just podman-reset-all-i-know-this-deletes-state
```

这会删除 `target/`、`.sccache`、全局 Mozilla sccache、`.run`、`.tmp` 选定测试产物，以及全部 Podman images/containers/volumes。后续 `just` 命令会按需重新编译和重新拉取资源镜像。

### 3.3.3 编译加速工具

仓库支持两类 Rust 编译优化：

1. `sccache`：缓存 rustc 编译结果，适合重复构建、分支切换和 CI-like 本地验证。
2. `cargo-hakari`：维护 `packages/workspace-hack`，减少 workspace 内依赖 feature 组合导致的重复编译。

安装/检查：

```bash
just setup-sccache
just setup-sccache-verify
just setup-hakari-verify
```

启用 `sccache` 可以使用一次性 just 命令，或显式环境变量。推荐先用一次性命令避免污染全局 shell：

```bash
just build-with-sccache
just build-release-with-sccache
just release-web-bff-with-sccache
```

这些命令使用项目级 `.sccache`，并只对当前 invocation 设置 `SCCACHE_DIR="$PWD/.sccache" RUSTC_WRAPPER=sccache`。

如果希望长期启用，再设置环境变量：

```bash
mise set RUSTC_WRAPPER=sccache
mise set SCCACHE_DIR "$PWD/.sccache"
```

如果 `just sccache-stats` 显示 `Compile requests 0`，说明当前构建没有走 `sccache`。

如需彻底删除 sccache 缓存：

```bash
just sccache-purge
```

当 `cargo-hakari` drift 时，使用独立变更刷新：

```bash
just hakari-update
```

### 3.3.4 Binary-first 与 Podman 资源容器

运行环境不负责编译 first-party Rust code。默认 single-VPS 路径是 binary-first：

```bash
just release-web-bff
just package-web-bff
just sops-export-env web-bff dev systemd-binary .run/web-bff.env
just smoke-web-bff-binary
```

Podman 在 single-VPS/local profile 中主要管理官方资源容器：

```bash
just podman-doctor
just podman-resources-up lite
just podman-resources-up surrealdb
just podman-resources-up standard
just podman-resources-status
just podman-resources-down
```

资源 preset 语义：

1. `lite`：不启动默认 Podman 资源；使用 embedded/local/in-process/managed backends。
2. `surrealdb`：只启动 SurrealDB 官方容器，用于本项目优先外部 DB lane。
3. `standard`：启动 SurrealDB、NATS、Valkey。
4. `full`：启动标准资源、MinIO、sqld/libSQL server 等 compose profile 中的重资源；observability/auth 仍由独立命令面控制。

如果团队选择 first-party app container，也应在本地/CI 先构建 binary，再构建只复制 artifact 的 runtime image，或把 prebuilt image 传到 VPS。不要在 VPS Podman 或 K3s 节点里 `cargo build --release`。

低配 VPS 的目标是避免分布式税：功能入口不消失，但 backend 可以是轻量 adapter 或关闭状态。语义限制必须明确，例如 Moka 是进程内 cache，stdout/journald 不是集中式 observability，in-process event bus 不是 durable broker。

若本地 Podman machine 只是运行资源容器，通常不需要为 Rust release image build 调到很高内存。macOS 上若确实做 image proof，可调整：

```bash
podman machine stop podman-machine-default
podman machine set --memory 6144 podman-machine-default
podman machine start podman-machine-default
```

### 3.3.5 后端优先的最小调试模式

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

单 VPS `systemd-binary` 或 `podman` profile 使用宿主机临时 env-file，而不是 `.env`：

```bash
just sops-export-env web-bff dev systemd-binary /run/axum-harness/web-bff.env
just sops-export-env web-bff dev podman /run/axum-harness/web-bff.env
```

原因：

1. 这与集群中的 `SOPS -> Kustomize/Flux` 路径保持环境变量形状一致。
2. 可以避免本地路径和交付路径分叉得过早。
3. 若要走本地 Podman auth 栈，可先 `source infra/local/generated/auth.env`，再用 host-process、`just sops-run` 或 `just sops-export-env` 启动 `web-bff`。

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
