# Operations control plane
#
# This file owns environment-coupled actions: deploy, migration, and host-level
# service management. These commands are intentionally separated from dev.just so
# risky or low-frequency operational paths do not pollute the default local loop.

# 部署到 k3s 集群
deploy-prod env='dev':
    @echo "Deploying to k3s environment: {{env}}..."
    cargo run -p repo-tools -- infra k3s deploy --env "{{env}}"

# 预览 k3s 部署变更
deploy-prod-dry-run env='dev':
    @echo "Dry run: deploying to k3s environment: {{env}}..."
    cargo run -p repo-tools -- infra k3s deploy --env "{{env}}" --dry-run client

# 初始化一台新的 k3s VPS
deploy-bootstrap-k3s:
    cargo run -p repo-tools -- infra k3s bootstrap

# 规划裸 VPS 的工具链 bootstrap
deploy-bootstrap-vps:
    cargo run -p repo-tools -- ops bootstrap-vps --plan

# 在目标服务器上部署并启动 axum API 服务
deploy-api:
    cargo run -p repo-tools -- ops service deploy --service axum-api

# 停止 API 服务
stop-api:
    cargo run -p repo-tools -- ops service stop --service axum-api

# 查看 API 服务日志
logs-api:
    cargo run -p repo-tools -- ops service logs --service axum-api

# 生成 systemd service 文件（需在目标服务器执行）
generate-service bin_path='' env_file='' user='root' group='root':
    @test -n "{{bin_path}}" || (echo 'Usage: just generate-service /path/to/binary /path/to/.env appuser appgroup' && exit 1)
    @test -n "{{env_file}}" || (echo 'Usage: just generate-service /path/to/binary /path/to/.env appuser appgroup' && exit 1)
    cargo run -p repo-tools -- generate-service --bin-path "{{bin_path}}" --env-file "{{env_file}}" --user "{{user}}" --group "{{group}}"
    @echo "Generated axum-api.service — copy to /etc/systemd/system/ and run: just deploy-api"

# 运行所有待执行 migration
migrate-up:
    @echo "=== Running migrations ==="
    cargo run -p repo-tools -- ops migrate --env local --direction up --dry-run

# 回滚上一条 migration
migrate-down:
    cargo run -p repo-tools -- ops migrate --env local --direction down --dry-run

# 查看 migration 状态
migrate-status:
    cargo run -p repo-tools -- ops migrate --env local --direction status --dry-run

# 创建新的 migration 文件
migrate-create name='':
    @test -n "{{name}}" || (echo "Usage: just migrate-create add_counter_index" && exit 1)
    cargo run -p repo-tools -- dev migration create "{{name}}"

# 重置数据库（仅限开发环境，危险操作）
migrate-reset:
    cargo run -p repo-tools -- ops migrate --env local --direction reset --dry-run
