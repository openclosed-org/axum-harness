# Clean — cleanup commands for local caches and build artifacts.
# Default cleanup is conservative: it avoids container volumes and global tool caches
# unless the command name makes that destructive scope explicit.

# 暴力清理所有 cargo 缓存（慢，下次全量重建）
clean:
    cargo clean

# 智能清理 7 天未访问的构建缓存
clean-sweep:
    cargo run -p repo-tools -- clean-sweep

# 本地磁盘维护：安全清理过期构建缓存、陈旧依赖缓存和大测试日志
clean-local-storage: clean-run-artifacts clean-tmp-truncate clean-sweep-deps clean-sweep
    @echo "✓ 本地缓存维护完成；如需删除 compose volumes，请显式运行 infra local down --volumes"

# 查看本地 Rust、SCCACHE、Podman 和 transient run artifacts 占用。
storage-report:
    @rtk du -sh target .sccache "$HOME/.cargo/registry" "$HOME/.cargo/git" "$HOME/Library/Caches/Mozilla.sccache" .run .tmp 2>/dev/null || true
    @podman system df 2>/dev/null || true

# 清理 .run transient smoke/env/log/cid/artifact 文件。不要在这里打印 secret 内容。
clean-run-artifacts:
    mkdir -p .run
    rm -rf .run/*
    @echo "✓ .run transient artifacts cleared"

# 清理不在当前 Cargo.toml 中的依赖缓存
clean-sweep-deps:
    cargo run -p repo-tools -- clean-sweep-deps

# 仅清理覆盖率产物
clean-coverage:
    just _require cargo-llvm-cov "cargo install cargo-llvm-cov"
    cargo llvm-cov clean

# 清理测试运行产物（.tmp 目录）
clean-tmp:
    @echo "清理 .tmp 测试产物..."
    rm -rf .tmp/phase3-smoke .tmp/run
    @echo "✓ .tmp 测试产物已清理"

# 截断 .tmp 中的大日志文件（超过 10MB 的截断到保留最后 5 万行）
clean-tmp-truncate:
    cargo run -p repo-tools -- cleanup-test-artifacts

# 全量清理：构建缓存 + 测试产物
clean-all: clean-tmp clean
    @echo "✓ 全量清理完成"

# 激进本地清理：删除 Cargo target、.run、.tmp、项目级 .sccache 和全局 Mozilla sccache。
clean-aggressive-local: clean-run-artifacts clean-tmp
    cargo clean
    rm -rf .sccache
    rm -rf "$HOME/Library/Caches/Mozilla.sccache"
    @echo "✓ aggressive local build/cache cleanup complete"

# 查看 sccache 缓存状态。若 Compile requests 为 0，说明当前构建未设置 RUSTC_WRAPPER=sccache。
sccache-stats:
    just _require sccache "cargo install sccache --locked"
    sccache --show-stats

# 清理 sccache 本地缓存。不会删除 Cargo target 或 Podman volumes。
sccache-clean:
    just _require sccache "cargo install sccache --locked"
    sccache --zero-stats
    sccache --stop-server
    @echo "sccache server stopped and stats reset. Delete SCCACHE_DIR manually only if disk pressure remains."

# 彻底删除 sccache 缓存目录。不会删除 Cargo target 或 Podman volumes。
sccache-purge:
    just _require sccache "cargo install sccache --locked"
    sccache --zero-stats || true
    sccache --stop-server || true
    rm -rf .sccache
    rm -rf "$HOME/Library/Caches/Mozilla.sccache"
    @echo "✓ sccache cache directories purged"
