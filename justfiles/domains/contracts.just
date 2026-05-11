# Contracts command surface.
# Shared DTO, event, SDK, and generated contract drift evidence live here.

# canonical entrypoint: contract verification surface
verify-contracts MODE='warn':
    cargo run -p repo-tools -- validate-contracts --mode {{MODE}}

# 生成合约类型绑定
typegen:
    moon run repo:typegen

# 合约漂移检查 — 检查 generated 目录是否与源码一致（不重新生成）
drift-check:
    cargo run -p repo-tools -- drift-check

# SDK 漂移检查 — 检查 packages/sdk/ 是否有未提交的变更
sdk-drift-check:
    cargo run -p repo-tools -- sdk-drift-check
