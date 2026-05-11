# Setup — toolchain installation and verification

# 安装/校验后端核心工具版本
setup:
    mise install

# 安装本地 NATS Server（用于本地开发和测试）
setup-nats:
    cargo run -p repo-tools -- setup-nats

# 一键初始化（工具 + 诊断）
bootstrap:
    mise install
    cargo run -p repo-tools -- doctor

# 检查后端核心工具链状态（不安装，只验证）
doctor:
    @echo "=== Toolchain ==="
    @mise current rust  || echo "MISSING: rust — run: just setup"
    @echo ""
    @echo "=== Versions ==="
    @cargo --version 2>/dev/null || echo "cargo not in PATH"

# 项目级健康检查（工具 + 配置文件）
doctor-full:
    cargo run -p repo-tools -- doctor

# 安装 sccache 编译缓存。安装不等于启用；启用需设置 RUSTC_WRAPPER=sccache。
setup-sccache:
    @echo "=== Installing sccache ==="
    cargo run -p repo-tools -- setup-sccache
    @echo ""
    @echo "=== sccache 安装完成 ==="
    @echo ""
    @echo "请配置环境变量（二选一）："
    @echo ""
    @echo "方案 A - 项目级 .env（推荐）："
    @echo "  1. 复制 .env.example 到 .env"
    @echo "  2. 取消注释 sccache 相关行"
    @echo ""
    @echo "方案 B - mise 环境变量："
    @echo "  mise set SCCACHE_DIR={{justfile_directory()}}/.sccache"
    @echo "  mise set RUSTC_WRAPPER=sccache"
    @echo ""
    @echo "配置完成后运行：just setup-sccache-verify"

# 验证 sccache 是否生效
setup-sccache-verify:
    @echo "=== sccache 验证 ==="
    @echo ""
    cargo run -p repo-tools -- setup-sccache-verify

# 安装 cargo-hakari 并刷新 workspace-hack。会修改生成区块，适合单独提交。
setup-hakari:
    cargo run -p repo-tools -- setup-hakari

# 验证 hakari 是否生效
setup-hakari-verify:
    @echo "=== hakari status ==="
    cargo run -p repo-tools -- setup-hakari-verify
    cargo hakari generate --diff
    cargo hakari manage-deps --dry-run

# 安装覆盖率工具（一次性）
setup-coverage:
    cargo run -p repo-tools -- setup-coverage

# 检查工具链版本
toolchain-check:
    cargo run -p repo-tools -- toolchain-check

# internal helper: require a local binary before running a dependent recipe
_require TOOL CMD:
    cargo run -p repo-tools -- require-tool "{{TOOL}}" "{{CMD}}"
