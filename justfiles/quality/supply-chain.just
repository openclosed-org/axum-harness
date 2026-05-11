# Supply-chain command surface.
# These recipes are quality evidence, but remain separate from default backend-core
# verification because they may need external tools or image inputs.

# Trivy 漏洞扫描（默认扫描整个项目文件系统，可指定 IMAGE）
scan-vuln IMAGE='' EXIT_CODE='0':
    cargo run -p repo-tools -- scan-vuln --image "{{IMAGE}}" --exit-code {{EXIT_CODE}}

# Syft SBOM 生成（默认生成 JSON 格式，输出到 stdout 或文件）
gen-sbom IMAGE='' FORMAT='json' OUTPUT='-':
    cargo run -p repo-tools -- gen-sbom --image "{{IMAGE}}" --format "{{FORMAT}}" --output "{{OUTPUT}}"

# Cosign 镜像签名（需要 COSIGN_EXPERIMENTAL=1 时用 keyless 模式）
sign-image IMAGE:
    cargo run -p repo-tools -- sign-image {{IMAGE}}

# Cosign 镜像验证
verify-image IMAGE:
    cargo run -p repo-tools -- verify-image {{IMAGE}}

# 一键跑全量供应链检查（本地 gate）
supply-chain:
    @echo "=== Supply Chain Gate ==="
    just scan-vuln '' 0
    just gen-sbom
    @echo "=== Supply Chain Gate PASSED ==="
