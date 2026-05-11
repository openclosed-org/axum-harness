# Justfile — 统一人类/Agent 命令入口
# 工具链由 mise 管理,任务编排由 moon 负责
# Justfile 只暴露稳定、可读性高的接口
#
# 命令面分层：
# - canonical: 默认推荐的人类入口，帮助页优先展示
# - internal: 供组合 recipe 复用的内部工具，不建议直接作为日常入口
# 语义约定：
# - check-*    低成本静态/编译检查
# - validate-* 规则、模型、元数据一致性校验
# - test-*     可执行测试
# - verify-*   组合验证或更强证据入口
# - gate-*     生命周期/治理门禁入口
set export  # 导出环境变量到子进程

# 模块导入
# justfiles/* 是命令面的一部分，不应静默缺失；这里保持显式导入和稳定顺序。
# 主轴：setup -> dev -> build -> verify -> gates -> ops -> clean
import 'justfiles/setup.just'
import 'justfiles/dev.just'
import 'justfiles/build.just'
import 'justfiles/verify.just'
import 'justfiles/gates.just'
import 'justfiles/ops.just'
import 'justfiles/clean.just'
import 'justfiles/podman.just'

# 副轴：platform / secrets / local cluster profiles / template / skills
import 'justfiles/platform.just'
import 'justfiles/sops.just'
import 'justfiles/k3d.just'
import 'justfiles/multipass.just'
import 'justfiles/template.just'
import 'justfiles/skills.just'

# ── 默认行为 / 导航 ──────────────────────────────────────────

default: help

help:
    @printf "\n"
    @printf "axum-harness command surface\n"
    @printf "===========================\n"
    @printf "\n"
    @printf "语义约定\n"
    @printf "  check-*    低成本静态/编译检查\n"
    @printf "  validate-* 规则、模型、元数据一致性校验\n"
    @printf "  test-*     可执行测试\n"
    @printf "  verify-*   组合验证或更强证据入口\n"
    @printf "  gate-*     生命周期/治理门禁入口\n"
    @printf "\n"
    @printf "高频入口\n"
    @printf "  just setup\n"
    @printf "  just doctor\n"
    @printf "  just dev\n"
    @printf "  just test\n"
    @printf "  just check-backend-primary\n"
    @printf "  just gate-ci-single-node\n"
    @printf "  just smoke-local-k3d\n"
    @printf "  just verify\n"
    @printf "\n"
    @printf "分组帮助\n"
    @printf "  just help-dev\n"
    @printf "  just help-verify\n"
    @printf "  just help-ops\n"
    @printf "  just help-platform\n"
    @printf "  just help-secrets\n"
    @printf "  just help-podman\n"
    @printf "  just help-local-clusters\n"
    @printf "  just help-template\n"
    @printf "  just help-all\n"
    @printf "\n"

help-dev:
    @printf "\n开发 / 本地运行\n"
    @printf "  just dev                      默认 web-bff 开发循环\n"
    @printf "  just dev-api                  启动 API 开发\n"
    @printf "  just deploy-dev               启动本地 core infra\n"
    @printf "  just status-dev               查看本地 infra 状态\n"
    @printf "  just logs-dev SERVICE=nats    跟随本地 infra 指定服务日志\n"
    @printf "  just dev-workers              启动本地 workers\n"
    @printf "  just status-workers           查看 worker 后台运行状态\n"
    @printf "  just health-workers           查看 worker 健康状态\n"
    @printf "  just ps                       查看本地进程状态\n"
    @printf "\n"

help-verify:
    @printf "\n验证 / 质量 / 门禁\n"
    @printf "  just fmt                      格式检查\n"
    @printf "  just lint                     Clippy/Lint\n"
    @printf "  just typecheck                编译型检查\n"
    @printf "  just test                     默认测试\n"
    @printf "  just check-backend-primary    默认 backend-core 低成本静态检查\n"
    @printf "  just verify                   repo 级默认验证\n"
    @printf "  just verify-contracts warn\n"
    @printf "  just drift-check              generated contract 漂移检查\n"
    @printf "  just boundary-check           架构边界检查\n"
    @printf "  just validate-publish-intent strict\n"
    @printf "  just audit-app-shell-boundary dry-run\n"
    @printf "  just gate-existence MODE=warn\n"
    @printf "  just gate-ci-single-node\n"
    @printf "  just gate-local-k3d\n"
    @printf "  just gate-release             release 门禁（RELEASE_TYPE=major 可声明 breaking）\n"
    @printf "\n"

help-ops:
    @printf "\n运维 / 部署 / 迁移\n"
    @printf "  just migrate-status           查看 migration 状态\n"
    @printf "  just release-web-bff          本地/CI 构建 web-bff release binary\n"
    @printf "  just release-web-bff-with-sccache 使用项目级 sccache 构建 release binary\n"
    @printf "  just package-web-bff          打包 binary + sha256 到 .run/artifacts\n"
    @printf "  just smoke-web-bff-binary     用本地 binary + SOPS env-file 跑 /healthz smoke\n"
    @printf "  just migrate-up               执行 migration（dry-run）\n"
    @printf "  just deploy-prod dev          部署到 k3s\n"
    @printf "  just deploy-prod-dry-run      预览 k3s 部署\n"
    @printf "  just generate-service ...     生成 systemd service\n"
    @printf "  just logs-api                 查看宿主机 API 日志\n"
    @printf "\n"

help-platform:
    @printf "\n平台 / 模型 / 生成产物\n"
    @printf "  just validate-platform        校验 platform models\n"
    @printf "  just validate-platform-json   JSON 形式输出 platform 校验结果\n"
    @printf "  just validate-state strict\n"
    @printf "  just validate-workflows strict\n"
    @printf "  just validate-contract-drift  platform 与 contracts 漂移检查\n"
    @printf "  just generate-platform-catalog 生成 platform catalog\n"
    @printf "  just platform-capabilities     列出 platform capability models\n"
    @printf "  just verify-replay MODE=strict\n"
    @printf "  just verify-generated-artifacts 生成产物基线校验\n"
    @printf "  just platform-doctor          平台全量健康检查\n"
    @printf "\n"

help-secrets:
    @printf "\nSecrets / SOPS\n"
    @printf "  just sops-gen-age-key\n"
    @printf "  just sops-show-age-key\n"
    @printf "  just sops-edit web-bff dev\n"
    @printf "  just sops-run web-bff dev 'cargo run -p web-bff' local_real\n"
    @printf "  just sops-run web-bff dev 'cargo run -p web-bff' external_single_node\n"
    @printf "  just sops-export-env web-bff dev systemd-binary /run/axum-harness/web-bff.env\n"
    @printf "  just sops-export-env web-bff dev podman /run/axum-harness/web-bff.env\n"
    @printf "  just sops-verify-counter-db-credentials dev\n"
    @printf "  just sops-reconcile dev\n"
    @printf "\n"

help-podman:
    @printf "\nPodman / Resource Containers\n"
    @printf "  just podman-doctor                  查看 Podman 资源和磁盘状态\n"
    @printf "  just podman-ensure                  检查 Podman 可达性\n"
    @printf "  just podman-resources-up lite       lite preset，不启动重资源容器\n"
    @printf "  just podman-resources-up surrealdb  只启动 SurrealDB 官方容器\n"
    @printf "  just podman-resources-up standard   启动 SurrealDB/NATS/Valkey\n"
    @printf "  just podman-resources-up full       启动完整资源 compose profiles\n"
    @printf "  just podman-resources-status        查看资源容器状态\n"
    @printf "  just podman-resources-down          停止资源容器，保留 volumes\n"
    @printf "  just podman-export-web-bff-env      可选 prebuilt app container env-file\n"
    @printf "  just podman-image-proof-web-bff     可选 Dockerfile proof，不是 VPS 部署路径\n"
    @printf "  just podman-smoke-prebuilt-web-bff  可选 prebuilt image /healthz smoke\n"
    @printf "  just podman-disk                    查看 Podman 详细磁盘占用\n"
    @printf "  just podman-prune-build-cache       清理 dangling build layers，保留 volumes\n"
    @printf "  just podman-prune-stopped-containers 清理 stopped containers，保留 volumes\n"
    @printf "  just podman-reset-all-i-know-this-deletes-state 清空 Podman images/containers/volumes\n"
    @printf "  just storage-report                 查看本机/Podman 存储占用\n"
    @printf "\n"

help-local-clusters:
    @printf "\n本地集群 / Profile Gate\n"
    @printf "  just gate-ci-single-node       GitHub CI/CD 对齐的单机向基础门禁\n"
    @printf "  just smoke-local-k3d           Colima + K3d 低资源 Kubernetes smoke\n"
    @printf "  just gate-local-k3d            Colima + K3d 本地完整 profile 门禁\n"
    @printf "  just k3d-up                    启动 1 server + 2 agents 的 K3d 集群\n"
    @printf "  just k3d-apply-infra           应用 namespace/secrets/addons\n"
    @printf "  just k3d-test-surrealdb-persistence\n"
    @printf "  just multipass-k3s-plan        打印 1.0 前 Multipass + K3s 手动方案\n"
    @printf "\n"

help-template:
    @printf "\nTemplate / Repo Maintenance\n"
    @printf "  just template-init backend-core dry-run\n"
    @printf "  just audit-backend-core dry-run\n"
    @printf "  just semver-check\n"
    @printf "  just skills-list\n"
    @printf "  just storage-report\n"
    @printf "  just clean-run-artifacts\n"
    @printf "  just clean-aggressive-local\n"
    @printf "  just sccache-purge\n"
    @printf "  just clean-local-storage\n"
    @printf "\n"

help-all:
    @just --list
