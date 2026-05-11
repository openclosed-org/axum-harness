# Build control plane
#
# This file owns artifact creation and compile-oriented package operations. It is
# intentionally narrower than verification: build recipes produce binaries or
# perform direct compile actions, while broader quality evidence lives in verify.just.

# Build entire workspace
build:
    moon run repo:build

# Build entire workspace with project-local sccache enabled for this invocation.
build-with-sccache:
    just _require sccache "cargo install sccache --locked"
    mkdir -p .sccache
    SCCACHE_DIR="$PWD/.sccache" RUSTC_WRAPPER=sccache moon run repo:build

# Build workspace in release mode
build-release:
    cargo build --workspace --release

# Build workspace release artifacts with project-local sccache enabled.
build-release-with-sccache:
    just _require sccache "cargo install sccache --locked"
    mkdir -p .sccache
    SCCACHE_DIR="$PWD/.sccache" RUSTC_WRAPPER=sccache cargo build --workspace --release

# Build a single crate
build-pkg PACKAGE='':
    @if [ -z "{{PACKAGE}}" ]; then \
        echo "Usage: just build-pkg <package-name>"; \
        exit 1; \
    fi
    cargo build -p {{PACKAGE}}

# Cross-compile for Linux musl (static binary)
build-linux-musl:
    cargo build --target x86_64-unknown-linux-musl --release

# Cross-compile for macOS (aarch64)
build-macos-aarch64:
    cargo build --target aarch64-apple-darwin --release

# Build API server only
build-api:
    cargo build -p web-bff --release

# Build the default web-bff release binary for binary-first host deployment.
release-web-bff:
    cargo build -p web-bff --bin web-bff --release

# Build the default web-bff release binary with project-local sccache enabled.
release-web-bff-with-sccache:
    just _require sccache "cargo install sccache --locked"
    mkdir -p .sccache
    SCCACHE_DIR="$PWD/.sccache" RUSTC_WRAPPER=sccache cargo build -p web-bff --bin web-bff --release

# Package the built web-bff binary with a checksum. Build outside runtime hosts.
package-web-bff OUT='.run/artifacts': release-web-bff
    mkdir -p "{{OUT}}"
    cp target/release/web-bff "{{OUT}}/web-bff"
    shasum -a 256 "{{OUT}}/web-bff" > "{{OUT}}/web-bff.sha256"
    @echo "Packaged {{OUT}}/web-bff"
    @echo "Checksum {{OUT}}/web-bff.sha256"

# Smoke a prebuilt web-bff binary with an exported env-file and local /healthz.
smoke-web-bff-binary BIN='target/release/web-bff' ENV_FILE='.run/web-bff.env' PORT='39110':
    @test -x "{{BIN}}" || (echo "Missing executable {{BIN}}. Run: just release-web-bff"; exit 1)
    @test -f "{{ENV_FILE}}" || (echo "Missing env-file {{ENV_FILE}}. Run: just sops-export-env web-bff dev systemd-binary {{ENV_FILE}}"; exit 1)
    mkdir -p .run
    @pid_file=.run/web-bff-binary.pid; log_file=.run/web-bff-binary.log; health_file=.run/web-bff-binary-health.json; \
      set -a; . "{{ENV_FILE}}"; set +a; \
      env -u APP_TURSO_URL -u APP_TURSO_AUTH_TOKEN APP_SERVER_HOST=127.0.0.1 APP_SERVER_PORT={{PORT}} APP_DATABASE_URL=file:./.run/web-bff-binary.db APP_RUNTIME_PROFILE=development "{{BIN}}" > "$log_file" 2>&1 & \
      pid=$!; printf "%s\n" "$pid" > "$pid_file"; \
      ok=1; \
      for i in $(seq 1 30); do \
        if curl -fsS http://127.0.0.1:{{PORT}}/healthz > "$health_file"; then ok=0; break; fi; \
        sleep 1; \
      done; \
      kill "$pid" >/dev/null 2>&1 || true; wait "$pid" >/dev/null 2>&1 || true; \
      if [ "$ok" -ne 0 ]; then echo "web-bff binary smoke failed. Logs: $log_file"; exit "$ok"; fi; \
      rtk read "$health_file"

# 检查 storage-turso 编译
check-storage-turso:
    cargo check -p web-bff

# Analyze binary size
bloat:
    cargo bloat --release --crates

# Check all crates compile without linking
check:
    moon run repo:check

# Check cargo-hakari workspace-hack drift without editing files.
hakari:
    cargo hakari generate --diff
    cargo hakari manage-deps --dry-run

# Refresh cargo-hakari workspace-hack. This edits generated hakari output.
hakari-update:
    cargo hakari generate
    cargo hakari manage-deps --yes
