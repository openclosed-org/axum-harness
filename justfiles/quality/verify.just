# Verification — aggregate quality entrypoints.
# Domain-specific checks live under justfiles/domains/*.just; lifecycle gates live
# under quality/gates.just. This file keeps the public verify surface stable.

# 格式化代码（默认 backend lane）
fmt:
    moon run repo:fmt

# 修复格式
fmt-fix:
    moon run repo:format-fix

# Lint（默认 backend lane）
lint:
    moon run repo:quality-lint

# 类型检查（默认 backend lane）
typecheck:
    moon run repo:typecheck

# 架构边界检查（features / contracts 依赖约束）
boundary-check:
    moon run repo:boundary-check

# 完整质量门禁（fmt + lint + check + test）
verify:
    moon run repo:verify

# 日常开发：快速单元测试（无覆盖率）
test:
    moon run repo:test-unit

# 使用 nextest 运行测试（推荐）
test-nextest:
    cargo nextest run --workspace

# cargo-hack feature powerset 检查
test-hack:
    cargo hack check --workspace --feature-powerset

# cargo-mutants 变异测试
test-mutants:
    cargo mutants --workspace

# 集成测试（被 --ignore 标记的）
test-integration:
    cargo test --workspace -- --ignored

# 所有 Rust 测试套件（nextest + coverage + hack）
test-all-rust:
    moon run repo:test-all-rust

# 本地覆盖率：生成 LCOV（供 CI/Codecov 用）
test-coverage:
    just _require cargo-llvm-cov "cargo install cargo-llvm-cov"
    moon run repo:test-coverage

# 本地覆盖率：生成 HTML 并在浏览器打开
test-coverage-html:
    just _require cargo-llvm-cov "cargo install cargo-llvm-cov"
    moon run repo:test-coverage-html

# 清理覆盖率产物
test-coverage-clean:
    just _require cargo-llvm-cov "cargo install cargo-llvm-cov"
    moon run repo:test-coverage-clean
