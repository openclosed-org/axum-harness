# Podman resource-container control plane.
# Podman is for opt-in official resource containers by default. First-party
# Rust deployables are binary-first and should be built outside runtime hosts.

_podman_image := 'axum-harness-web-bff:dev'
_podman_container := 'axum-harness-web-bff-dev'
_podman_env := '.run/podman-web-bff.env'

# Show Podman machine resources, disk usage, and local image state.
podman-doctor:
    just _require podman "brew install podman"
    @podman info >/dev/null || (echo "Podman is not reachable. Start it with: podman machine start"; exit 1)
    @echo "=== Podman machine ==="
    @podman machine inspect podman-machine-default 2>/dev/null || true
    @echo "=== Podman disk ==="
    podman system df
    @echo "=== first-party proof images, if any ==="
    @podman images localhost/{{_podman_image}} 2>/dev/null || true

# Ensure Podman is reachable before resource operations.
podman-ensure:
    just _require podman "brew install podman"
    @podman info >/dev/null || (echo "Podman is not reachable. Start it with: podman machine start"; exit 1)

# Start official resource containers by preset. Presets: lite, surrealdb, standard, full.
podman-resources-up PRESET='lite':
    just podman-ensure
    @case "{{PRESET}}" in \
      lite) echo "lite preset: no Podman resource containers are required by default." ;; \
      surrealdb) podman compose -f infra/docker/compose/core.yaml -p tauri-sveltekit-dev --profile surrealdb up -d surrealdb ;; \
      standard) podman compose -f infra/docker/compose/core.yaml -p tauri-sveltekit-dev --profile standard up -d surrealdb nats valkey ;; \
      full) podman compose -f infra/docker/compose/core.yaml -p tauri-sveltekit-dev --profile full --profile standard --profile surrealdb up -d ;; \
      *) echo "Unknown preset '{{PRESET}}'. Use: lite, surrealdb, standard, full"; exit 1 ;; \
    esac

# Stop official resource containers. Volumes are preserved.
podman-resources-down:
    just podman-ensure
    podman compose -f infra/docker/compose/core.yaml -p tauri-sveltekit-dev --profile full --profile standard --profile surrealdb down

# Show official resource container status.
podman-resources-status:
    just podman-ensure
    podman compose -f infra/docker/compose/core.yaml -p tauri-sveltekit-dev --profile full --profile standard --profile surrealdb ps

# Follow logs for a resource container.
podman-resources-logs SERVICE='surrealdb':
    just podman-ensure
    podman compose -f infra/docker/compose/core.yaml -p tauri-sveltekit-dev --profile full --profile standard --profile surrealdb logs -f {{SERVICE}}

# Export SOPS env-file for an optional prebuilt Podman app container.
podman-export-web-bff-env ENV='dev' OUTPUT=_podman_env:
    mkdir -p .run
    just sops-export-env web-bff {{ENV}} podman {{OUTPUT}}

# Optional proof only: build the web-bff Dockerfile locally. Not a single-VPS deployment path.
podman-image-proof-web-bff IMAGE=_podman_image:
    just podman-doctor
    @echo "Image proof only: do not use VPS Podman or K3s nodes as Cargo builders."
    podman build -f infra/docker/Dockerfile.web-bff -t {{IMAGE}} .

# Optional proof only: run a prebuilt/proof web-bff image and check /healthz.
podman-smoke-prebuilt-web-bff IMAGE=_podman_image ENV_FILE=_podman_env PORT='39111':
    just _require podman "brew install podman"
    @test -f "{{ENV_FILE}}" || (echo "Missing env-file {{ENV_FILE}}. Run: just podman-export-web-bff-env"; exit 1)
    @podman rm -f {{_podman_container}} >/dev/null 2>&1 || true
    podman run -d --name {{_podman_container}} --env-file {{ENV_FILE}} -e APP_SERVER_HOST=0.0.0.0 -e APP_SERVER_PORT=3010 -e APP_DATABASE_URL=file:/tmp/web-bff.db -e APP_TURSO_URL -e APP_TURSO_AUTH_TOKEN -e APP_RUNTIME_PROFILE=development -p {{PORT}}:3010 {{IMAGE}} > .run/podman-web-bff.cid
    @ok=1; \
      for i in $(seq 1 30); do \
        if curl -fsS http://127.0.0.1:{{PORT}}/healthz > .run/podman-web-bff-health.json; then ok=0; break; fi; \
        sleep 1; \
      done; \
      podman logs {{_podman_container}} > .run/podman-web-bff.log 2>&1 || true; \
      podman rm -f {{_podman_container}} >/dev/null 2>&1 || true; \
      if [ "${ok}" -ne 0 ]; then echo "Podman prebuilt web-bff smoke failed. Logs: .run/podman-web-bff.log"; exit "${ok}"; fi; \
      rtk read .run/podman-web-bff-health.json

# Show Podman storage details.
podman-disk:
    just podman-ensure
    podman system df -v

# Conservative cleanup: remove dangling images/build cache, keep containers and volumes.
podman-prune-build-cache:
    just podman-ensure
    podman image prune -f

# Remove stopped containers, keep images and volumes.
podman-prune-stopped-containers:
    just podman-ensure
    podman container prune -f

# More aggressive cleanup: remove unused images/containers/networks, keep volumes by default.
podman-prune-unused:
    just podman-ensure
    podman system prune -f

# Explicit destructive cleanup: remove resource volumes too.
podman-prune-volumes-i-know-this-deletes-state:
    just podman-ensure
    podman volume prune -f

# Destructive reset: remove all Podman containers, images, networks, build cache, and volumes.
podman-reset-all-i-know-this-deletes-state:
    just podman-ensure
    @podman rm -af >/dev/null 2>&1 || true
    podman system prune -a -f --volumes
    podman volume prune -f
    podman image prune -a -f
