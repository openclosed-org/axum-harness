use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::cli::{
    ContainerRuntime, InfraArgs, InfraAuthCommand, InfraCommand, InfraK3sBootstrapArgs,
    InfraK3sCommand, InfraK3sDeployArgs, InfraLocalCommand, InfraLocalDownArgs, InfraLocalLogsArgs,
    InfraLocalStatusArgs, InfraLocalUpArgs, InfraObservabilityCommand, InfraObservabilityDownArgs,
    InfraObservabilityLogsArgs, InfraObservabilityStatusArgs, InfraObservabilityUpArgs,
    KubectlDryRunMode,
};
use crate::support::{
    Operation, OperationPhase, has_tool, read, require_tool, run_capture, run_inherit,
    wait_for_port, workspace_root, write,
};

const AUTH_COMPOSE_FILE: &str = "infra/docker/compose/auth.yaml";
const LOCAL_COMPOSE_FILE: &str = "infra/docker/compose/core.yaml";
const LOCAL_PROJECT_NAME: &str = "tauri-sveltekit-dev";
const OBSERVABILITY_PROJECT_NAME: &str = "tauri-sveltekit-observability";
const AUTH_STATE_DIR: &str = "infra/local/state";
const AUTH_GENERATED_DIR: &str = "infra/local/generated";
const AUTH_ENV_FILE: &str = "infra/local/generated/auth.env";
const OPENFGA_MODEL_FILE: &str = "fixtures/authz-tuples/counter-model.openfga.json";
const RAUTHY_ISSUER: &str = "http://localhost:8082/auth/v1/";
const RAUTHY_INTROSPECTION_URL: &str = "http://localhost:8082/auth/v1/oidc/introspect";
const K3S_OVERLAYS_DIR: &str = "infra/k3s/overlays";
const OBSERVABILITY_COMPOSE_FILE: &str = "infra/docker/compose/observability.yaml";

pub(crate) fn run(args: InfraArgs) -> Result<()> {
    match args.command {
        InfraCommand::Auth(args) => run_auth(args.command),
        InfraCommand::Local(args) => run_local(args.command),
        InfraCommand::K3s(args) => match args.command {
            InfraK3sCommand::Deploy(args) => k3s_deploy(args),
            InfraK3sCommand::Bootstrap(args) => k3s_bootstrap(args),
        },
    }
}

fn run_local(command: InfraLocalCommand) -> Result<()> {
    let root = workspace_root()?;
    match command {
        InfraLocalCommand::Up(args) => local_up(&root, args),
        InfraLocalCommand::Down(args) => local_down(&root, args),
        InfraLocalCommand::Status(args) => local_status(&root, args),
        InfraLocalCommand::Logs(args) => local_logs(&root, args),
        InfraLocalCommand::Observability(args) => run_observability(&root, args.command),
    }
}

fn local_up(root: &Path, args: InfraLocalUpArgs) -> Result<()> {
    let runtime = resolve_runtime(args.runtime)?;
    let mut compose_args = local_compose_base_args(args.profile);
    compose_args.push("up".to_string());
    if args.detach {
        compose_args.push("-d".to_string());
    }

    println!("Starting local infrastructure with {}", runtime.label());
    run_local_compose(root, runtime, compose_args)?;
    print_local_connection_info();
    Ok(())
}

fn local_down(root: &Path, args: InfraLocalDownArgs) -> Result<()> {
    let runtime = resolve_runtime(args.runtime)?;
    let mut compose_args = local_compose_base_args(Vec::new());
    compose_args.push("down".to_string());
    if args.volumes {
        println!("WARNING: --volumes removes local compose volumes for {LOCAL_PROJECT_NAME}");
        compose_args.push("--volumes".to_string());
    }

    println!("Stopping local infrastructure with {}", runtime.label());
    run_local_compose(root, runtime, compose_args)
}

fn local_status(root: &Path, args: InfraLocalStatusArgs) -> Result<()> {
    let runtime = resolve_runtime(args.runtime)?;
    let mut compose_args = local_compose_base_args(Vec::new());
    compose_args.push("ps".to_string());
    if args.json {
        compose_args.push("--format".to_string());
        compose_args.push("json".to_string());
    }

    run_local_compose(root, runtime, compose_args)?;
    if !args.json {
        print_local_connection_info();
    }
    Ok(())
}

fn local_logs(root: &Path, args: InfraLocalLogsArgs) -> Result<()> {
    let runtime = resolve_runtime(args.runtime)?;
    let mut compose_args = local_compose_base_args(Vec::new());
    compose_args.push("logs".to_string());
    if args.follow {
        compose_args.push("-f".to_string());
    }
    if let Some(service) = args.service {
        compose_args.push(service);
    }

    run_local_compose(root, runtime, compose_args)
}

fn run_observability(root: &Path, command: InfraObservabilityCommand) -> Result<()> {
    match command {
        InfraObservabilityCommand::Up(args) => observability_up(root, args),
        InfraObservabilityCommand::Down(args) => observability_down(root, args),
        InfraObservabilityCommand::Status(args) => observability_status(root, args),
        InfraObservabilityCommand::Logs(args) => observability_logs(root, args),
    }
}

fn observability_up(root: &Path, args: InfraObservabilityUpArgs) -> Result<()> {
    let runtime = resolve_runtime(args.runtime)?;
    let podman_socket = observability_podman_socket(runtime)?;
    let mut subcommand = vec!["up"];
    if args.detach {
        subcommand.push("-d");
    }
    println!("Starting observability stack with {}", runtime.label());
    run_observability_compose(root, runtime, &subcommand, podman_socket.as_deref())?;
    print_observability_connection_info();
    Ok(())
}

fn observability_down(root: &Path, args: InfraObservabilityDownArgs) -> Result<()> {
    let runtime = resolve_runtime(args.runtime)?;
    println!("Stopping observability stack with {}", runtime.label());
    run_observability_compose(root, runtime, &["down"], None)
}

fn observability_status(root: &Path, args: InfraObservabilityStatusArgs) -> Result<()> {
    let runtime = resolve_runtime(args.runtime)?;
    let mut subcommand = vec!["ps"];
    if args.json {
        subcommand.push("--format");
        subcommand.push("json");
    }
    run_observability_compose(root, runtime, &subcommand, None)?;
    if !args.json {
        print_observability_connection_info();
    }
    Ok(())
}

fn observability_logs(root: &Path, args: InfraObservabilityLogsArgs) -> Result<()> {
    let runtime = resolve_runtime(args.runtime)?;
    let mut subcommand = vec!["logs"];
    if args.follow {
        subcommand.push("-f");
    }
    if let Some(service) = args.service.as_deref() {
        subcommand.push(service);
    }
    run_observability_compose(root, runtime, &subcommand, None)
}

fn observability_podman_socket(runtime: ContainerRuntime) -> Result<Option<String>> {
    if runtime != ContainerRuntime::Podman {
        return Ok(None);
    }
    if let Ok(socket) = std::env::var("PODMAN_SOCKET") {
        if !socket.trim().is_empty() {
            return Ok(Some(socket));
        }
    }
    let output = run_capture(
        "podman",
        &[
            "machine",
            "inspect",
            "--format",
            "{{.ConnectionInfo.PodmanSocket.Path}}",
        ],
        None,
    )?;
    if output.success && !output.output.trim().is_empty() {
        return Ok(Some(output.output.trim().to_string()));
    }
    Ok(None)
}

fn resolve_runtime(requested: Option<ContainerRuntime>) -> Result<ContainerRuntime> {
    if let Some(runtime) = requested {
        require_tool(runtime.binary(), &format!("install {}", runtime.label()))?;
        return Ok(runtime);
    }

    if has_tool("docker") {
        return Ok(ContainerRuntime::Docker);
    }
    if has_tool("podman") {
        println!("docker not found; falling back to podman compose");
        return Ok(ContainerRuntime::Podman);
    }

    bail!(
        "missing container runtime. install Docker Desktop or Podman Desktop, or pass --runtime docker|podman"
    )
}

fn local_compose_base_args(profiles: Vec<String>) -> Vec<String> {
    let mut args = vec!["compose".to_string()];
    for profile in profiles {
        args.push("--profile".to_string());
        args.push(profile);
    }
    args.extend([
        "-f".to_string(),
        LOCAL_COMPOSE_FILE.to_string(),
        "-p".to_string(),
        LOCAL_PROJECT_NAME.to_string(),
    ]);
    args
}

fn run_local_compose(root: &Path, runtime: ContainerRuntime, args: Vec<String>) -> Result<()> {
    let compose_file = root.join(LOCAL_COMPOSE_FILE);
    if !compose_file.is_file() {
        bail!("local compose file not found: {}", compose_file.display());
    }

    let borrowed_args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let code = run_inherit(runtime.binary(), &borrowed_args, Some(root))?;
    if code != 0 {
        bail!("{} failed with status {code}", runtime.label());
    }
    Ok(())
}

fn run_observability_compose(
    root: &Path,
    runtime: ContainerRuntime,
    subcommand: &[&str],
    podman_socket: Option<&str>,
) -> Result<()> {
    let compose_file = root.join(OBSERVABILITY_COMPOSE_FILE);
    if !compose_file.is_file() {
        bail!(
            "observability compose file not found: {}",
            compose_file.display()
        );
    }

    let mut command = Command::new(runtime.binary());
    command
        .current_dir(root)
        .args([
            "compose",
            "-f",
            OBSERVABILITY_COMPOSE_FILE,
            "-p",
            OBSERVABILITY_PROJECT_NAME,
        ])
        .args(subcommand)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(socket) = podman_socket {
        command.env("PODMAN_SOCKET", socket);
    }
    let status = command
        .status()
        .with_context(|| format!("failed to run {} observability compose", runtime.label()))?;
    let code = status.code().unwrap_or(1);
    if code != 0 {
        bail!("{} failed with status {code}", runtime.label());
    }
    Ok(())
}

fn print_local_connection_info() {
    println!();
    println!("Optional resource container endpoints when their profiles are enabled:");
    println!("  SurrealDB: http://localhost:8000 (--profile surrealdb|standard|full)");
    println!("  sqld/libSQL HTTP: http://localhost:8080 (--profile full)");
    println!("  sqld/libSQL gRPC: grpc://localhost:5001 (--profile full)");
    println!("  NATS: nats://localhost:4222 (--profile standard|full)");
    println!("  NATS monitor: http://localhost:8222 (--profile standard|full)");
    println!("  Valkey: redis://localhost:6379 (--profile standard|full)");
    println!("  MinIO API: http://localhost:9000 (--profile full)");
    println!("  MinIO Console: http://localhost:9001 (minioadmin/minioadmin, --profile full)");
}

fn print_observability_connection_info() {
    println!();
    println!("Observability endpoints:");
    println!("  OpenObserve UI: http://localhost:5080 (admin@localhost / admin)");
}

fn run_auth(command: InfraAuthCommand) -> Result<()> {
    let root = workspace_root()?;
    ensure_auth_dirs(&root)?;
    match command {
        InfraAuthCommand::Up => auth_compose(&root, &["up", "-d"]),
        InfraAuthCommand::Down => auth_compose(&root, &["down"]),
        InfraAuthCommand::Status => auth_compose(&root, &["ps"]),
        InfraAuthCommand::Logs => auth_compose(&root, &["logs", "-f"]),
        InfraAuthCommand::Bootstrap => auth_bootstrap(&root),
    }
}

fn ensure_auth_dirs(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join(AUTH_STATE_DIR))
        .context("failed to create local auth state dir")?;
    fs::create_dir_all(root.join(AUTH_GENERATED_DIR))
        .context("failed to create local auth generated dir")?;
    Ok(())
}

fn auth_compose(root: &Path, args: &[&str]) -> Result<()> {
    require_tool("podman", "install Podman Desktop or podman CLI")?;
    let compose_file = root.join(AUTH_COMPOSE_FILE);
    if !compose_file.is_file() {
        bail!("auth compose file not found: {}", compose_file.display());
    }

    let mut full_args = vec!["compose", "-f", AUTH_COMPOSE_FILE];
    full_args.extend_from_slice(args);
    let code = run_inherit("podman", &full_args, Some(root))?;
    if code != 0 {
        bail!("podman compose failed with status {code}");
    }
    Ok(())
}

fn auth_bootstrap(root: &Path) -> Result<()> {
    require_tool("podman", "install Podman Desktop or podman CLI")?;
    ensure_auth_dirs(root)?;

    println!("Starting local auth stack...");
    auth_compose(root, &["up", "-d"])?;
    wait_http("localhost", 8081, "OpenFGA")?;
    wait_http("localhost", 8082, "Rauthy")?;

    let openfga = bootstrap_openfga(root)?;
    write_rauthy_state(root)?;
    write_auth_env(root, &openfga)?;

    println!("Local auth stack is running.");
    println!("Next:");
    println!("1. source {AUTH_ENV_FILE}");
    println!("2. export APP_DATABASE_URL=file:./.data/web-bff.db");
    println!("3. cargo run -p web-bff");
    println!("Rauthy local reference IdP: {RAUTHY_ISSUER}");
    println!("OpenFGA API: http://localhost:8081");
    Ok(())
}

fn wait_http(host: &str, port: u16, name: &str) -> Result<()> {
    if wait_for_port(host, port, Duration::from_secs(120)) {
        Ok(())
    } else {
        bail!("timed out waiting for {name} at {host}:{port}")
    }
}

struct OpenFgaBootstrap {
    store_id: String,
    model_id: String,
}

fn bootstrap_openfga(root: &Path) -> Result<OpenFgaBootstrap> {
    let model_file = root.join(OPENFGA_MODEL_FILE);
    if !model_file.is_file() {
        bail!("OpenFGA model file not found: {}", model_file.display());
    }

    let store_resp = http_json(
        "POST",
        "http://localhost:8081/stores",
        &[],
        Some(r#"{"name":"local-counter-authz"}"#),
    )?;
    let store_id = json_string(&store_resp, &["id"])?;
    let model = read(&model_file)?;
    let model_resp = http_json(
        "POST",
        &format!("http://localhost:8081/stores/{store_id}/authorization-models"),
        &[],
        Some(&model),
    )?;
    let model_id = json_string(&model_resp, &["authorization_model_id"])?;

    write(
        root.join(AUTH_STATE_DIR).join("openfga.store_id"),
        &format!("{store_id}\n"),
    )?;
    write(
        root.join(AUTH_STATE_DIR).join("openfga.model_id"),
        &format!("{model_id}\n"),
    )?;

    println!("OpenFGA store: {store_id}");
    println!("OpenFGA model: {model_id}");
    Ok(OpenFgaBootstrap { store_id, model_id })
}

fn write_rauthy_state(root: &Path) -> Result<()> {
    write(
        root.join(AUTH_STATE_DIR).join("rauthy.issuer"),
        &format!("{RAUTHY_ISSUER}\n"),
    )?;
    write(
        root.join(AUTH_STATE_DIR).join("rauthy.introspection_url"),
        &format!("{RAUTHY_INTROSPECTION_URL}\n"),
    )?;
    Ok(())
}

fn write_auth_env(root: &Path, openfga: &OpenFgaBootstrap) -> Result<()> {
    write(
        root.join(AUTH_ENV_FILE),
        &format!(
            "APP_OIDC_ISSUER={RAUTHY_ISSUER}\nAPP_OIDC_AUDIENCE=web-bff-local\nAPP_OIDC_INTROSPECTION_URL={RAUTHY_INTROSPECTION_URL}\nAPP_AUTHZ_PROVIDER=openfga\nAPP_AUTHZ_ENDPOINT=http://localhost:8081\nAPP_AUTHZ_STORE_ID={}\nAPP_AUTHZ_MODEL_ID={}\n",
            openfga.store_id, openfga.model_id,
        ),
    )?;
    println!("Wrote {AUTH_ENV_FILE}");
    Ok(())
}

fn http_json(
    method: &str,
    url: &str,
    headers: &[String],
    body: Option<&str>,
) -> Result<serde_json::Value> {
    let client = reqwest::blocking::Client::new();
    let method = reqwest::Method::from_bytes(method.as_bytes()).context("invalid HTTP method")?;
    let mut request = client.request(method, url);
    for header in headers {
        let (name, value) = header
            .split_once(':')
            .with_context(|| format!("invalid header: {header}"))?;
        request = request.header(name.trim(), value.trim());
    }
    if let Some(body) = body {
        request = request
            .header("content-type", "application/json")
            .body(body.to_string());
    }
    let text = request
        .send()
        .with_context(|| format!("failed HTTP request to {url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP request failed for {url}"))?
        .text()
        .context("failed to read HTTP response")?;
    serde_json::from_str(&text).with_context(|| format!("failed to parse JSON response from {url}"))
}

fn json_string(value: &serde_json::Value, path: &[&str]) -> Result<String> {
    let mut current = value;
    for segment in path {
        current = current
            .get(*segment)
            .with_context(|| format!("JSON response missing {}", path.join(".")))?;
    }
    current
        .as_str()
        .map(str::to_string)
        .with_context(|| format!("JSON response field {} is not a string", path.join(".")))
}

fn k3s_deploy(args: InfraK3sDeployArgs) -> Result<()> {
    let operation = Operation::new("infra-k3s-deploy", args.dry_run.is_none());
    let root = workspace_root()?;
    let overlay = k3s_overlay(&root, &args.env)?;
    operation.phase(
        OperationPhase::Plan,
        format!("deploy overlay {}", overlay.display()),
    );
    preflight_k3s_deploy(&overlay, args.dry_run)?;
    operation.phase(OperationPhase::Preflight, "k3s deploy preflight passed");

    println!("Deploying to k3s environment: {}", args.env);
    let render = run_capture(
        "kustomize",
        &[
            "build",
            "--load-restrictor",
            "LoadRestrictionsNone",
            overlay_str(&overlay)?,
        ],
        Some(&root),
    )?;
    if !render.success {
        bail!("kustomize build failed: {}", render.error);
    }
    let resource_count = render
        .output
        .lines()
        .filter(|line| line.starts_with("kind:"))
        .count();
    println!("Rendered manifest for {}", args.env);
    println!("Resource kind lines: {resource_count}");

    let mut apply_args = vec!["apply", "-f", "-"];
    if let Some(mode) = args.dry_run {
        apply_args.push(mode.arg());
        println!("Dry run mode: {}", mode.arg());
        if mode == KubectlDryRunMode::Client {
            println!("Client dry-run rendered manifest only; no kubectl API call was made.");
            println!("Dry run complete; skipping post-deploy verification");
            return Ok(());
        }
    }
    operation.phase(
        OperationPhase::Execute,
        "apply rendered manifest via kubectl",
    );
    kubectl_apply_manifest(&render.output, &apply_args, Some(&root))?;

    if args.dry_run.is_none() && !args.skip_verify {
        operation.phase(OperationPhase::Verify, "wait for deployment availability");
        verify_k3s_deploy()?;
    } else if args.dry_run.is_some() {
        println!("Dry run complete; skipping post-deploy verification");
    }
    Ok(())
}

fn kubectl_apply_manifest(manifest: &str, args: &[&str], cwd: Option<&Path>) -> Result<()> {
    let mut command = Command::new("kubectl");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to start kubectl apply")?;
    child
        .stdin
        .as_mut()
        .context("failed to open kubectl stdin")?
        .write_all(manifest.as_bytes())
        .context("failed to pass rendered manifest to kubectl")?;
    let status = child.wait().context("failed to wait for kubectl apply")?;
    if !status.success() {
        bail!(
            "kubectl apply failed with status {}",
            status.code().unwrap_or(1)
        );
    }
    Ok(())
}

fn preflight_k3s_deploy(overlay: &Path, dry_run: Option<KubectlDryRunMode>) -> Result<()> {
    require_tool("kubectl", "install kubectl and configure cluster access")?;
    require_tool("kustomize", "install kustomize")?;
    if !overlay.join("kustomization.yaml").is_file() {
        bail!("kustomization.yaml not found in {}", overlay.display());
    }
    if dry_run == Some(KubectlDryRunMode::Client) {
        println!("k3s deploy preflight passed without cluster check for client dry-run");
        return Ok(());
    }
    let cluster = run_capture("kubectl", &["cluster-info"], None)?;
    if !cluster.success {
        bail!("cannot connect to Kubernetes cluster: {}", cluster.error);
    }
    println!("k3s deploy preflight passed");
    Ok(())
}

fn verify_k3s_deploy() -> Result<()> {
    println!("Verifying deployment...");
    let wait_code = run_inherit(
        "kubectl",
        &[
            "wait",
            "--for=condition=available",
            "--timeout=120s",
            "deployment",
            "--all",
            "-n",
            "app",
        ],
        None,
    )?;
    if wait_code != 0 {
        println!(
            "WARNING: some deployments may not be ready yet. Check with: kubectl get pods -n app"
        );
    }
    let pods_code = run_inherit("kubectl", &["get", "pods", "-n", "app", "-o", "wide"], None)?;
    if pods_code != 0 {
        bail!("kubectl get pods failed with status {pods_code}");
    }
    let svc_code = run_inherit("kubectl", &["get", "svc", "-n", "app"], None)?;
    if svc_code != 0 {
        bail!("kubectl get svc failed with status {svc_code}");
    }
    Ok(())
}

fn k3s_overlay(root: &Path, env: &str) -> Result<PathBuf> {
    let overlay = root.join(K3S_OVERLAYS_DIR).join(env);
    if !overlay.is_dir() {
        let available = available_k3s_envs(root)?;
        bail!(
            "unknown k3s environment: {env}. available environments: {}",
            available.join(", ")
        );
    }
    Ok(overlay)
}

fn available_k3s_envs(root: &Path) -> Result<Vec<String>> {
    let mut envs = Vec::new();
    for entry in fs::read_dir(root.join(K3S_OVERLAYS_DIR)).context("failed to read k3s overlays")? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            envs.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    envs.sort();
    Ok(envs)
}

fn overlay_str(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn k3s_bootstrap(args: InfraK3sBootstrapArgs) -> Result<()> {
    let operation = Operation::new(
        "infra-k3s-bootstrap",
        args.apply && args.i_understand_this_modifies_host,
    );
    println!("k3s bootstrap plan:");
    println!("1. verify OS, architecture, memory, ports, and privilege state");
    println!("2. install k3s server with host-level system changes");
    println!("3. configure kubectl from /etc/rancher/k3s/k3s.yaml");
    println!("4. verify cluster health");
    operation.phase(OperationPhase::Plan, "host mutation path is guarded");
    k3s_bootstrap_preflight()?;
    operation.phase(OperationPhase::Preflight, "bootstrap preflight passed");

    if !(args.apply && args.i_understand_this_modifies_host) {
        println!("Preflight complete; not modifying host.");
        println!("To execute, rerun with --apply --i-understand-this-modifies-host.");
        return Ok(());
    }

    operation.phase(
        OperationPhase::Execute,
        "host mutation intentionally blocked",
    );
    bail!(
        "k3s bootstrap execution is intentionally not implemented yet; the safe Phase 6 boundary is plan/preflight only"
    )
}

fn k3s_bootstrap_preflight() -> Result<()> {
    println!("OS: {}", std::env::consts::OS);
    println!("ARCH: {}", std::env::consts::ARCH);
    if !matches!(std::env::consts::ARCH, "x86_64" | "aarch64") {
        bail!("unsupported architecture: {}", std::env::consts::ARCH);
    }
    if cfg!(target_os = "linux") {
        let euid = run_capture("id", &["-u"], None)?;
        if euid.success {
            let is_root = euid.output.trim() == "0";
            println!("root: {is_root}");
        }
        if Path::new("/proc/meminfo").is_file() {
            let meminfo = read("/proc/meminfo")?;
            if let Some(line) = meminfo.lines().find(|line| line.starts_with("MemTotal:")) {
                println!("{line}");
            }
        }
    } else {
        println!(
            "WARNING: k3s bootstrap target is Linux; current OS is {}",
            std::env::consts::OS
        );
    }
    if let Ok(k3s) = run_capture("k3s", &["--version"], None) {
        if k3s.success {
            println!(
                "Existing k3s detected: {}",
                k3s.output.lines().next().unwrap_or("k3s present")
            );
        }
    }
    println!("Required ports to check manually if running apply: 6443, 80, 443, 8472");
    Ok(())
}
