# Local K3d profile — low-resource Kubernetes verification on macOS via Colima.
#
# Default resource target for an 8GB/256GB Mac mini:
#   colima start --cpu 4 --memory 4 --disk 30 --runtime docker --dns 1.1.1.1 --dns 8.8.8.8

K3D_CLUSTER := 'axh-local'
COLIMA_DOCKER_HOST := 'unix://${HOME}/.colima/default/docker.sock'

# Start Colima and create a 1-server + 2-agent K3d cluster.
k3d-up CLUSTER=K3D_CLUSTER:
    just _require colima "brew install colima"
    just _require docker "mise install"
    just _require k3d "mise install"
    just _require kubectl "mise install"
    bash -lc 'colima status >/dev/null 2>&1 || colima start --cpu 4 --memory 4 --disk 30 --runtime docker --dns 1.1.1.1 --dns 8.8.8.8'
    bash -lc 'docker context inspect colima >/dev/null 2>&1 && docker context use colima || DOCKER_HOST="{{COLIMA_DOCKER_HOST}}" docker info >/dev/null'
    bash -lc 'export DOCKER_HOST="{{COLIMA_DOCKER_HOST}}"; k3d cluster get {{CLUSTER}} >/dev/null 2>&1 || k3d cluster create {{CLUSTER}} --servers 1 --agents 2 --api-port 127.0.0.1:6443 --port "8080:80@loadbalancer" --k3s-arg "--disable=traefik@server:*" --k3s-arg "--disable=servicelb@server:*"'
    kubectl config use-context k3d-{{CLUSTER}}

# Show local K3d status.
k3d-status:
    kubectl get nodes -o wide
    kubectl get pods -A

# Delete the local K3d cluster. Does not prune Colima/Docker globally.
k3d-down CLUSTER=K3D_CLUSTER:
    k3d cluster delete {{CLUSTER}}

# Show Docker disk usage before any manual cleanup.
k3d-disk-usage:
    docker system df

# Create namespaces, reconcile dev secrets, and apply addon manifests.
k3d-apply-infra:
    kubectl create namespace app --dry-run=client -o yaml | kubectl apply -f -
    kubectl create namespace app-dev --dry-run=client -o yaml | kubectl apply -f -
    just sops-reconcile dev
    kubectl apply -k infra/kubernetes/addons/

# Validate expected local K3d nodes are Ready.
k3d-test-nodes:
    kubectl get nodes -o wide
    bash -lc 'test "$(kubectl get nodes --no-headers | wc -l | tr -d " ")" -eq 3'
    kubectl wait node --all --for=condition=Ready --timeout=180s

# Wait for SurrealDB StatefulSet readiness.
k3d-test-surrealdb-ready:
    kubectl -n app rollout status statefulset/surrealdb --timeout=240s
    kubectl -n app wait pod/surrealdb-0 --for=condition=Ready --timeout=240s

# Verify cluster DNS and HTTP reachability for the SurrealDB Service.
k3d-test-surrealdb-service-dns:
    kubectl -n app run surrealdb-dns-check --rm -i --restart=Never --image=curlimages/curl:8.16.0 --command -- curl -sf http://surrealdb.app.svc.cluster.local:8000/health

# Verify SurrealDB StatefulSet PVC survives pod restart.
k3d-test-surrealdb-persistence:
    printf "UPSERT persistence_probe:k3d SET marker = 'survives-k3d-pod-restart', checked_by = 'k3d-local-profile', created_at = time::now();\n" | kubectl -n app run surrealdb-persistence-write --rm -i --restart=Never --image=surrealdb/surrealdb:v3.0.5 --command -- /surreal sql --endpoint http://surrealdb.app.svc.cluster.local:8000 --user root --pass root --ns axh --db main --pretty --hide-welcome
    kubectl -n app delete pod surrealdb-0 --wait=true
    kubectl -n app rollout status statefulset/surrealdb --timeout=240s
    kubectl -n app wait pod/surrealdb-0 --for=condition=Ready --timeout=240s
    printf "SELECT * FROM persistence_probe:k3d;\n" | kubectl -n app run surrealdb-persistence-read --rm -i --restart=Never --image=surrealdb/surrealdb:v3.0.5 --command -- /surreal sql --endpoint http://surrealdb.app.svc.cluster.local:8000 --user root --pass root --ns axh --db main --pretty --hide-welcome | grep "survives-k3d-pod-restart"

# Verify dev SOPS secret made it into the cluster.
k3d-test-secrets:
    kubectl -n app get secret surrealdb-secrets
    kubectl -n app get secret surrealdb-secrets -o jsonpath='{.data.SURREALDB_USER}' | grep -q .
    kubectl -n app get secret surrealdb-secrets -o jsonpath='{.data.SURREALDB_PASS}' | grep -q .

# Worker Kubernetes Deployments are not declared yet; keep this as source-level evidence.
k3d-test-worker-smoke:
    just verify-replay strict
    kubectl apply --dry-run=client -f infra/kubernetes/base/configmaps/outbox-relay-worker-config.yaml

# Observability cluster stack is not declared yet; keep this as declared-config evidence.
k3d-test-observability-smoke:
    just validate-observability
