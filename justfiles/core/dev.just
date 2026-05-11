# Development control plane
#
# This file owns the local inner loop: app/server startup, local infra lifecycle,
# process visibility, and worker runtime helpers. It intentionally keeps host-local
# development concerns together so the day-to-day path does not leak into ops or
# verification surfaces.

# 启动默认后端开发（web-bff）
dev:
    moon run repo:dev-api

# 启动 API 开发（web-bff）
dev-api:
    moon run repo:dev-api

# 启动本地轻量资源 preset。默认不启动重资源容器。
deploy-dev:
    @echo "Starting local lite resource preset..."
    just podman-resources-up lite

# 启动本地 standard 资源 preset + observability
deploy-dev-full:
    @echo "Starting dev infrastructure + observability..."
    just podman-resources-up standard
    cargo run -p repo-tools -- infra local observability up

# 启动本地 auth stack（Rauthy + OpenFGA local reference）
auth-up:
    @echo "Starting local auth stack..."
    cargo run -p repo-tools -- infra auth up

# 初始化本地 auth stack 并生成运行时环境骨架
auth-bootstrap:
    @echo "Bootstrapping local auth stack..."
    cargo run -p repo-tools -- infra auth bootstrap

# 停止本地 auth stack
auth-down:
    @echo "Stopping local auth stack..."
    cargo run -p repo-tools -- infra auth down

# 查看本地 auth stack 状态
auth-status:
    cargo run -p repo-tools -- infra auth status

# 跟随本地 auth stack 日志
auth-logs:
    cargo run -p repo-tools -- infra auth logs

# 停止本地 core infra
stop-dev:
    @echo "Stopping dev infrastructure..."
    cargo run -p repo-tools -- infra local observability down
    cargo run -p repo-tools -- infra local down

# 跟随本地 infra 指定服务日志
logs-dev SERVICE='nats':
    cargo run -p repo-tools -- infra local logs --follow --service "{{SERVICE}}"

# 跟随本地 infra 全量日志
logs-dev-all:
    cargo run -p repo-tools -- infra local logs --follow

# 查看本地 core infra 状态
status-dev:
    cargo run -p repo-tools -- infra local status

# 查看本项目相关进程状态
ps:
    cargo run -p repo-tools -- dev process status

# 停止所有本项目启动的开发进程
stop:
    cargo run -p repo-tools -- dev process stop

# 停止指定端口的进程
stop-port PORT='':
    @test -n "{{PORT}}" || (echo "Usage: just stop-port 3010" && exit 1)
    cargo run -p repo-tools -- dev process stop-port {{PORT}}

# 清理当前用户的孤儿/无响应进程
clean-orphans:
    cargo run -p repo-tools -- dev process clean-orphans

# 显示端口占用详情
ports:
    cargo run -p repo-tools -- dev process ports

# 启动所有 workers（后台运行）
dev-workers:
    cargo run -p repo-tools -- dev worker start

# 停止所有 workers
stop-workers:
    cargo run -p repo-tools -- dev worker stop

# 查看 workers 后台运行状态
status-workers:
    cargo run -p repo-tools -- dev worker status

# 启动单个 worker
run-worker WORKER='':
    @test -n "{{WORKER}}" || (echo "Usage: just run-worker WORKER=outbox-relay" && exit 1)
    cargo run -p repo-tools -- dev worker run {{WORKER}}

# 查看 workers 健康状态
health-workers:
    cargo run -p repo-tools -- dev worker health
