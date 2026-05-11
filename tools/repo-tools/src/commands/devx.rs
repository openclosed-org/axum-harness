use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use xshell::{Shell, cmd};

use crate::cli::{
    DevAgeKeyArgs, DevAgeKeyCommand, DevArgs, DevCommand, DevCreateMigrationArgs, DevMigrationArgs,
    DevMigrationCommand, DevProcessArgs, DevProcessCommand, DevStopPortArgs, DevWorkerArgs,
    DevWorkerCommand, DevWorkerRunArgs, DevWorkerStartArgs, GenSbomArgs, GenerateServiceArgs,
    ImageRefArgs, K6BaselineArgs, RequireToolArgs, ScanVulnArgs,
};
use crate::support;
use crate::support::{
    Operation, OperationPhase, collect_files_with_extension, has_tool, normalize_slashes, read,
    require_tool, run_capture, run_inherit, strip_rust_comments, user_home_dir, workspace_root,
    write,
};

const AGE_KEY_RELATIVE_PATH: &str = ".config/sops/age/key.txt";
const DEFAULT_STOP_PATTERNS: &[&str] = &["cargo run -p web-bff", "web-bff", "moon run repo:dev"];
const WORKER_STATE_DIR: &str = ".tmp/dev/workers";
const WORKERS: &[WorkerSpec] = &[
    WorkerSpec::new("outbox-relay", 3030),
    WorkerSpec::new("indexer", 3031),
    WorkerSpec::new("projector", 3032),
    WorkerSpec::new("scheduler", 3033),
    WorkerSpec::new("sync-reconciler", 3034),
];

pub(crate) fn run_dev(args: DevArgs) -> Result<()> {
    match args.command {
        DevCommand::AgeKey(args) => run_age_key(args),
        DevCommand::Process(args) => run_process(args),
        DevCommand::Migration(args) => run_migration(args),
        DevCommand::Worker(args) => run_worker(args),
    }
}

fn run_age_key(args: DevAgeKeyArgs) -> Result<()> {
    match args.command {
        DevAgeKeyCommand::Generate(args) => generate_age_key(args.overwrite),
        DevAgeKeyCommand::Show => show_age_key(),
    }
}

fn run_process(args: DevProcessArgs) -> Result<()> {
    match args.command {
        DevProcessCommand::Status => process_status(),
        DevProcessCommand::Ports => process_ports(),
        DevProcessCommand::Stop => stop_default_processes(),
        DevProcessCommand::StopPort(args) => stop_port(args),
        DevProcessCommand::CleanOrphans => clean_orphans(),
    }
}

fn run_migration(args: DevMigrationArgs) -> Result<()> {
    match args.command {
        DevMigrationCommand::Create(args) => create_migration(args),
    }
}

fn run_worker(args: DevWorkerArgs) -> Result<()> {
    match args.command {
        DevWorkerCommand::Start(args) => start_workers(args),
        DevWorkerCommand::Stop => stop_workers(),
        DevWorkerCommand::Status => worker_status(),
        DevWorkerCommand::Health => worker_health(),
        DevWorkerCommand::Run(args) => run_single_worker(args),
    }
}

fn generate_age_key(overwrite: bool) -> Result<()> {
    require_tool("age-keygen", "install age via your package manager or mise")?;
    let key_path = age_key_path()?;
    if key_path.exists() && !overwrite {
        bail!(
            "age key already exists at {}. rerun with --overwrite after reviewing the risk",
            key_path.display()
        );
    }
    if let Some(parent) = key_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let key_arg = key_path
        .to_str()
        .with_context(|| format!("path is not valid UTF-8: {}", key_path.display()))?;
    let output = run_capture("age-keygen", &["-o", key_arg], None)?;
    if !output.success {
        bail!("age-keygen failed: {}", output.error);
    }
    println!("Generated age key: {}", key_path.display());
    show_age_key()
}

fn show_age_key() -> Result<()> {
    let key_path = age_key_path()?;
    let content = read(&key_path).with_context(|| {
        format!(
            "age key not found at {}. generate one with `cargo run -p repo-tools -- dev age-key generate`",
            key_path.display()
        )
    })?;
    let public_key = content
        .lines()
        .find_map(|line| line.strip_prefix("# public key: "))
        .context("age key file is missing the public key comment")?;
    println!("Age public key:");
    println!("{public_key}");
    println!();
    println!("Key file: {}", key_path.display());
    Ok(())
}

fn age_key_path() -> Result<std::path::PathBuf> {
    Ok(user_home_dir()?.join(AGE_KEY_RELATIVE_PATH))
}

fn process_status() -> Result<()> {
    let root = workspace_root()?;
    println!("=== Project process status ===");
    print_port_process(&root, 3010, "web-bff")?;
    for worker in WORKERS {
        print_port_process(&root, worker.port, worker.package)?;
    }
    Ok(())
}

fn process_ports() -> Result<()> {
    let root = workspace_root()?;
    println!("=== Project port occupancy ===");
    print_port_process(&root, 3010, "web-bff")?;
    for worker in WORKERS {
        print_port_process(&root, worker.port, worker.package)?;
    }
    Ok(())
}

fn stop_default_processes() -> Result<()> {
    println!("Stopping default development processes...");
    for pattern in DEFAULT_STOP_PATTERNS {
        stop_processes_matching(pattern)?;
    }
    println!("Done.");
    Ok(())
}

fn print_port_process(root: &std::path::Path, port: u16, label: &str) -> Result<()> {
    #[cfg(windows)]
    {
        return print_port_process_windows(port, label);
    }
    #[cfg(not(windows))]
    {
        print_port_process_unix(root, port, label)
    }
}

#[cfg(not(windows))]
fn print_port_process_unix(root: &std::path::Path, port: u16, label: &str) -> Result<()> {
    println!("Port {port} ({label}):");
    let port_string = format!(":{port}");
    let output = run_capture("lsof", &["-i", &port_string], Some(root))?;
    if !output.success || output.output.trim().is_empty() {
        println!("  [FREE]");
        return Ok(());
    }
    for line in output.output.lines() {
        println!("  {line}");
    }
    Ok(())
}

#[cfg(windows)]
fn print_port_process_windows(port: u16, label: &str) -> Result<()> {
    println!("Port {port} ({label}):");
    let lines = windows_port_lines(port)?;
    if lines.is_empty() {
        println!("  [FREE]");
        return Ok(());
    }
    for line in lines {
        println!("  {line}");
    }
    Ok(())
}

fn stop_port(args: DevStopPortArgs) -> Result<()> {
    #[cfg(windows)]
    {
        return stop_port_windows(args.port);
    }
    #[cfg(not(windows))]
    {
        stop_port_unix(args)
    }
}

#[cfg(not(windows))]
fn stop_port_unix(args: DevStopPortArgs) -> Result<()> {
    let root = workspace_root()?;
    let port_string = format!(":{}", args.port);
    let lsof = run_capture("lsof", &["-ti", &port_string], Some(&root))?;
    if !lsof.success || lsof.output.trim().is_empty() {
        println!("Nothing is listening on port {}", args.port);
        return Ok(());
    }

    for pid in lsof
        .output
        .lines()
        .map(str::trim)
        .filter(|pid| !pid.is_empty())
    {
        let status = run_capture("kill", &["-TERM", pid], Some(&root))?;
        if !status.success {
            bail!(
                "failed to stop pid {pid} on port {}: {}",
                args.port,
                status.error
            );
        }
        println!("Stopped pid {pid} on port {}", args.port);
    }
    Ok(())
}

#[cfg(windows)]
fn stop_port_windows(port: u16) -> Result<()> {
    let pids = windows_port_pids(port)?;
    if pids.is_empty() {
        println!("Nothing is listening on port {port}");
        return Ok(());
    }

    for pid in pids {
        terminate_pid(pid)?;
        println!("Stopped pid {pid} on port {port}");
    }
    Ok(())
}

fn clean_orphans() -> Result<()> {
    #[cfg(windows)]
    {
        clean_orphans_windows()
    }
    #[cfg(not(windows))]
    {
        clean_orphans_unix()
    }
}

#[cfg(windows)]
fn clean_orphans_windows() -> Result<()> {
    println!("Cleaning non-responding user processes on Windows...");
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-Process | Where-Object { $_.Responding -eq $false -and $_.SessionId -eq (Get-Process -Id $PID).SessionId } | Select-Object -ExpandProperty Id",
        ])
        .output()
        .context("failed to inspect Windows processes")?;
    if !output.status.success() {
        bail!("failed to inspect Windows processes");
    }

    let pids = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|pid| !pid.is_empty())
        .collect::<Vec<_>>();
    if pids.is_empty() {
        println!("No non-responding processes found.");
        return Ok(());
    }

    for pid in pids {
        let status = Command::new("taskkill")
            .args(["/PID", pid, "/F"])
            .status()
            .with_context(|| format!("failed to stop pid {pid}"))?;
        if !status.success() {
            bail!("failed to stop pid {pid}");
        }
        println!("Stopped pid {pid}");
    }
    Ok(())
}

#[cfg(not(windows))]
fn clean_orphans_unix() -> Result<()> {
    require_tool("ps", "install procps or use a Unix-like shell")?;
    println!("Cleaning orphaned zombie processes for the current user...");
    let zombies = Command::new("ps")
        .args(["-axo", "pid=,stat="])
        .output()
        .context("failed to inspect process table")?;
    if !zombies.status.success() {
        bail!("failed to inspect process table");
    }

    let pids = String::from_utf8_lossy(&zombies.stdout)
        .lines()
        .filter_map(parse_zombie_pid)
        .collect::<Vec<_>>();
    if pids.is_empty() {
        println!("No zombie processes found.");
        return Ok(());
    }

    for pid in pids {
        let pid_text = pid.to_string();
        let status = run_capture("kill", &["-9", &pid_text], None)?;
        if !status.success {
            if status.error.contains("No such process") {
                println!("Skipped zombie pid {pid} (already exited)");
                continue;
            }
            bail!("failed to clean zombie pid {pid}: {}", status.error);
        }
        println!("Cleaned zombie pid {pid}");
    }
    Ok(())
}

#[cfg(not(windows))]
fn parse_zombie_pid(line: &str) -> Option<u32> {
    let mut parts = line.split_whitespace();
    let pid = parts.next()?.parse::<u32>().ok()?;
    let stat = parts.next()?;
    stat.starts_with('Z').then_some(pid)
}

fn create_migration(args: DevCreateMigrationArgs) -> Result<()> {
    let operation = Operation::new("dev-migration-create", true);
    operation.phase(
        OperationPhase::Plan,
        format!("prepare migration file for {}", args.name),
    );
    let root = workspace_root()?;
    let dir = root.join("ops/migrations/api");
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let timestamp = run_capture("date", &["+%Y%m%d%H%M%S"], Some(&root))?;
    if !timestamp.success || timestamp.output.trim().is_empty() {
        bail!("failed to generate migration timestamp");
    }
    let file = dir.join(format!(
        "{}_{}.sql",
        timestamp.output.trim(),
        sanitize_migration_name(&args.name)
    ));
    if file.exists() {
        bail!("migration already exists: {}", file.display());
    }
    operation.phase(
        OperationPhase::Execute,
        format!("create {}", normalize_slashes(file.strip_prefix(&root)?)),
    );
    write(&file, "")?;
    println!(
        "Created migration: {}",
        normalize_slashes(file.strip_prefix(&root)?)
    );
    Ok(())
}

fn sanitize_migration_name(name: &str) -> String {
    name.chars()
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' => ch,
            'A'..='Z' => ch.to_ascii_lowercase(),
            _ => '_',
        })
        .collect()
}

fn start_workers(args: DevWorkerStartArgs) -> Result<()> {
    let root = workspace_root()?;
    let state_dir = worker_state_dir(&root);
    fs::create_dir_all(&state_dir)
        .with_context(|| format!("failed to create {}", state_dir.display()))?;
    println!("Starting workers...");
    for worker in WORKERS {
        println!("  - {} (health: :{})", worker.binary(), worker.port);
    }
    println!();

    if args.attach {
        println!(
            "Attach mode only supports a single worker. Use `repo-tools dev worker run <name>`."
        );
        bail!("--attach is not supported for the full worker set");
    }

    for worker in WORKERS {
        stop_worker(&state_dir, *worker)?;
        spawn_background_worker(&root, &state_dir, *worker)?;
    }

    println!("All workers started in background.");
    println!("Use `cargo run -p repo-tools -- dev worker stop` to stop them.");
    Ok(())
}

fn stop_workers() -> Result<()> {
    let root = workspace_root()?;
    let state_dir = worker_state_dir(&root);
    println!("Stopping workers...");
    for worker in WORKERS {
        stop_worker(&state_dir, *worker)?;
    }
    println!("Done.");
    Ok(())
}

fn worker_status() -> Result<()> {
    let root = workspace_root()?;
    let state_dir = worker_state_dir(&root);
    println!("=== Worker Runtime Status ===");
    println!("State dir: {}", state_dir.display());
    for worker in WORKERS {
        let pid_path = worker_pid_path(&state_dir, *worker);
        let log_path = worker_log_path(&state_dir, *worker);
        let pid = read_pid(&pid_path)?;
        let running = pid.is_some_and(process_exists);
        println!();
        println!("{}:", worker.binary());
        match pid {
            Some(pid) if running => println!("  pid: {pid} [RUNNING]"),
            Some(pid) => println!("  pid: {pid} [STALE]"),
            None => println!("  pid: [NONE]"),
        }
        println!("  log: {}", log_path.display());
    }
    Ok(())
}

fn run_single_worker(args: DevWorkerRunArgs) -> Result<()> {
    let worker = worker_by_name(&args.worker)?;
    println!("Starting {}...", worker.binary());
    let root = workspace_root()?;
    let code = run_inherit("cargo", &["run", "-p", worker.binary()], Some(&root))?;
    if code != 0 {
        bail!("{} exited with status {code}", worker.binary());
    }
    Ok(())
}

fn worker_health() -> Result<()> {
    let client = Client::builder().build()?;
    println!("=== Worker Health ===");
    for worker in WORKERS {
        println!();
        println!("{} (:{}):", worker.binary(), worker.port);
        let url = format!("http://localhost:{}/healthz", worker.port);
        match client.get(&url).send() {
            Ok(response) => {
                let status = response.status();
                let body = response.text().unwrap_or_default();
                if status.is_success() {
                    println!("{}", format_health_body(&body));
                } else {
                    println!("[HTTP {}] {}", status.as_u16(), body.trim());
                }
            }
            Err(_) => println!("[UNREACHABLE]"),
        }
    }
    Ok(())
}

fn stop_processes_matching(pattern: &str) -> Result<()> {
    #[cfg(windows)]
    {
        return stop_processes_matching_windows(pattern);
    }
    #[cfg(not(windows))]
    {
        stop_processes_matching_unix(pattern)
    }
}

#[cfg(not(windows))]
fn stop_processes_matching_unix(pattern: &str) -> Result<()> {
    let output = run_capture("pkill", &["-f", pattern], None)?;
    if output.success {
        println!("  stopped pattern: {pattern}");
        return Ok(());
    }
    let stderr = output.error.trim();
    if stderr.is_empty() {
        println!("  pattern not running: {pattern}");
        return Ok(());
    }
    bail!("failed to stop pattern `{pattern}`: {stderr}")
}

#[cfg(windows)]
fn stop_processes_matching_windows(pattern: &str) -> Result<()> {
    let script = format!(
        "Get-CimInstance Win32_Process | Where-Object {{ $_.CommandLine -like '*{pattern}*' }} | Select-Object -ExpandProperty ProcessId"
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .context("failed to inspect Windows processes")?;
    if !output.status.success() {
        bail!("failed to inspect Windows processes for pattern `{pattern}`");
    }

    let pids = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect::<Vec<_>>();
    if pids.is_empty() {
        println!("  pattern not running: {pattern}");
        return Ok(());
    }

    for pid in pids {
        terminate_pid(pid)?;
    }
    println!("  stopped pattern: {pattern}");
    Ok(())
}

fn spawn_background_worker(root: &Path, state_dir: &Path, worker: WorkerSpec) -> Result<()> {
    let log_path = worker_log_path(state_dir, worker);
    let pid_path = worker_pid_path(state_dir, worker);
    let stdout = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open {}", log_path.display()))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("failed to clone {}", log_path.display()))?;

    let child = Command::new("cargo")
        .args(["run", "-p", worker.binary()])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .with_context(|| format!("failed to start {}", worker.binary()))?;

    write(&pid_path, &child.id().to_string())?;
    println!(
        "  started {} pid={} log={}",
        worker.binary(),
        child.id(),
        log_path.display()
    );
    Ok(())
}

fn stop_worker(state_dir: &Path, worker: WorkerSpec) -> Result<()> {
    let pid_path = worker_pid_path(state_dir, worker);
    if let Some(pid) = read_pid(&pid_path)? {
        if process_exists(pid) {
            terminate_pid(pid)?;
            println!("  stopped {} pid={}", worker.binary(), pid);
        } else {
            println!("  stale pid removed for {} ({})", worker.binary(), pid);
        }
        remove_if_exists(&pid_path)?;
        return Ok(());
    }

    stop_processes_matching(worker.binary())
}

fn worker_state_dir(root: &Path) -> PathBuf {
    root.join(WORKER_STATE_DIR)
}

fn worker_pid_path(state_dir: &Path, worker: WorkerSpec) -> PathBuf {
    state_dir.join(format!("{}.pid", worker.binary()))
}

fn worker_log_path(state_dir: &Path, worker: WorkerSpec) -> PathBuf {
    state_dir.join(format!("{}.log", worker.binary()))
}

fn read_pid(path: &Path) -> Result<Option<u32>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = read(path)?;
    let pid = content
        .trim()
        .parse::<u32>()
        .with_context(|| format!("invalid pid file {}", path.display()))?;
    Ok(Some(pid))
}

fn process_exists(pid: u32) -> bool {
    #[cfg(windows)]
    {
        process_exists_windows(pid)
    }
    #[cfg(not(windows))]
    {
        process_exists_unix(pid)
    }
}

#[cfg(not(windows))]
fn process_exists_unix(pid: u32) -> bool {
    run_capture("kill", &["-0", &pid.to_string()], None)
        .map(|outcome| outcome.success)
        .unwrap_or(false)
}

#[cfg(windows)]
fn process_exists_windows(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}")])
        .output()
        .map(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line.contains(&pid.to_string()))
        })
        .unwrap_or(false)
}

fn terminate_pid(pid: u32) -> Result<()> {
    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .with_context(|| format!("failed to stop pid {pid}"))?;
        if !status.success() {
            bail!("failed to stop pid {pid}");
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let status = run_capture("kill", &["-TERM", &pid.to_string()], None)?;
        if !status.success {
            bail!("failed to stop pid {pid}: {}", status.error);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn windows_port_lines(port: u16) -> Result<Vec<String>> {
    let output = Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .output()
        .context("failed to inspect Windows TCP ports")?;
    if !output.status.success() {
        bail!("failed to inspect Windows TCP ports");
    }
    let needle = format!(":{port}");
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("TCP") && line.contains(&needle))
        .map(ToOwned::to_owned)
        .collect())
}

#[cfg(windows)]
fn windows_port_pids(port: u16) -> Result<Vec<u32>> {
    let mut pids = windows_port_lines(port)?
        .into_iter()
        .filter_map(|line| line.split_whitespace().last()?.parse::<u32>().ok())
        .collect::<Vec<_>>();
    pids.sort_unstable();
    pids.dedup();
    Ok(pids)
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    }
    Ok(())
}

fn worker_by_name(name: &str) -> Result<&'static WorkerSpec> {
    WORKERS
        .iter()
        .find(|worker| worker.package == name || worker.binary() == name)
        .with_context(|| format!("unknown worker `{name}`"))
}

fn format_health_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "[EMPTY RESPONSE]".to_string();
    }
    if let Ok(json) = serde_json::from_str::<Value>(trimmed) {
        return serde_json::to_string_pretty(&json).unwrap_or_else(|_| trimmed.to_string());
    }
    trimmed.to_string()
}

#[derive(Clone, Copy)]
struct WorkerSpec {
    package: &'static str,
    port: u16,
}

impl WorkerSpec {
    const fn new(package: &'static str, port: u16) -> Self {
        Self { package, port }
    }

    fn binary(self) -> &'static str {
        match self.package {
            "outbox-relay" => "outbox-relay-worker",
            "indexer" => "indexer-worker",
            "projector" => "projector-worker",
            "scheduler" => "scheduler-worker",
            "sync-reconciler" => "sync-reconciler-worker",
            _ => self.package,
        }
    }
}

pub(crate) fn scan_vuln(args: ScanVulnArgs) -> Result<()> {
    require_tool("trivy", "install trivy via your package manager or mise")?;
    let root = workspace_root()?;
    let exit_code = args.exit_code.to_string();
    if args.image.trim().is_empty() {
        println!("=== Trivy filesystem scan ===");
        let target = root.to_string_lossy().to_string();
        let code = run_inherit(
            "trivy",
            &[
                "fs",
                "--severity",
                "HIGH,CRITICAL",
                "--exit-code",
                &exit_code,
                &target,
            ],
            Some(&root),
        )?;
        if code != 0 {
            bail!("trivy fs failed with status {code}");
        }
        return Ok(());
    }

    println!("=== Trivy image scan: {} ===", args.image);
    let code = run_inherit(
        "trivy",
        &[
            "image",
            "--severity",
            "HIGH,CRITICAL",
            "--exit-code",
            &exit_code,
            &args.image,
        ],
        Some(&root),
    )?;
    if code != 0 {
        bail!("trivy image failed with status {code}");
    }
    Ok(())
}

pub(crate) fn gen_sbom(args: GenSbomArgs) -> Result<()> {
    require_tool("syft", "install syft via your package manager or mise")?;
    let root = workspace_root()?;
    let target = if args.image.trim().is_empty() {
        println!("=== Syft SBOM: workspace ===");
        format!("dir:{}", root.display())
    } else {
        println!("=== Syft SBOM: image {} ===", args.image);
        args.image.clone()
    };
    let output_format = if args.output == "-" {
        args.format.clone()
    } else {
        format!("{}={}", args.format, args.output)
    };
    let code = run_inherit("syft", &[&target, "-o", &output_format], Some(&root))?;
    if code != 0 {
        bail!("syft failed with status {code}");
    }
    Ok(())
}

pub(crate) fn sign_image(args: ImageRefArgs) -> Result<()> {
    require_tool("cosign", "install cosign via your package manager or mise")?;
    println!("=== Cosign sign: {} ===", args.image);
    let code = run_inherit("cosign", &["sign", &args.image], None)?;
    if code != 0 {
        bail!("cosign sign failed with status {code}");
    }
    Ok(())
}

pub(crate) fn verify_image(args: ImageRefArgs) -> Result<()> {
    require_tool("cosign", "install cosign via your package manager or mise")?;
    println!("=== Cosign verify: {} ===", args.image);
    let code = run_inherit("cosign", &["verify", &args.image], None)?;
    if code != 0 {
        bail!("cosign verify failed with status {code}");
    }
    Ok(())
}

pub(crate) fn generate_service(args: GenerateServiceArgs) -> Result<()> {
    let working_directory = workspace_root()?;
    let content = format!(
        "[Unit]\nDescription=Axum API Service\nAfter=network.target\n\n[Service]\nType=simple\nUser={}\nGroup={}\nWorkingDirectory={}\nEnvironmentFile={}\nExecStart={}\nRestart=on-failure\nRestartSec=5\nNoNewPrivileges=true\nProtectSystem=strict\nReadWritePaths=/var/lib/axum-api\n\n[Install]\nWantedBy=multi-user.target\n",
        args.user,
        args.group,
        working_directory.display(),
        args.env_file.display(),
        args.bin_path.display()
    );
    write(working_directory.join("axum-api.service"), &content)?;
    println!("Generated axum-api.service");
    Ok(())
}
pub(crate) fn setup_nats() -> Result<()> {
    let root = workspace_root()?;
    let nats_dir = root.join("tools/nats");
    let nats_bin = if cfg!(windows) {
        nats_dir.join("nats-server.exe")
    } else {
        nats_dir.join("nats-server")
    };
    if nats_bin.exists() {
        println!("✓ nats-server already present: {}", nats_bin.display());
        return Ok(());
    }

    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(windows) {
        "windows"
    } else {
        bail!("unsupported os")
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    };
    let version = "2.10.22";
    println!("Platform: {os}/{arch}");
    fs::create_dir_all(&nats_dir)?;

    if os == "windows" {
        let file_name = format!("nats-server-v{version}-{os}-{arch}.zip");
        let url = format!(
            "https://github.com/nats-io/nats-server/releases/download/v{version}/{file_name}"
        );
        let archive = nats_dir.join(&file_name);
        support::download_to(&url, &archive)?;
        let extracted = support::extract_zip_file(&archive, &nats_dir, "nats-server.exe")?;
        fs::rename(extracted, &nats_bin).ok();
        fs::remove_file(&archive).ok();
    } else {
        let file_name = format!("nats-server-v{version}-{os}-{arch}.tar.gz");
        let url = format!(
            "https://github.com/nats-io/nats-server/releases/download/v{version}/{file_name}"
        );
        let archive = nats_dir.join(&file_name);
        support::download_to(&url, &archive)?;
        support::extract_tar_gz(&archive, &nats_dir, Some("nats-server"))?;
        fs::remove_file(&archive).ok();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&nats_bin)?.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&nats_bin, permissions)?;
        }
    }

    println!("✓ nats-server installed: {}", nats_bin.display());
    Ok(())
}

pub(crate) fn cleanup_test_artifacts() -> Result<()> {
    let root = workspace_root()?;
    let tmp_dir = root.join(".tmp");
    if !tmp_dir.exists() {
        return Ok(());
    }
    const MAX_SIZE: u64 = 10 * 1024 * 1024;
    const KEEP_LINES: usize = 50_000;
    for log_file in collect_files_with_extension(&tmp_dir, "log") {
        let metadata = fs::metadata(&log_file)?;
        if metadata.len() <= MAX_SIZE {
            continue;
        }
        println!(
            "  截断 {} (当前 {} bytes)",
            normalize_slashes(log_file.strip_prefix(&root)?),
            metadata.len()
        );
        let content = read(&log_file)?;
        let kept = content
            .lines()
            .rev()
            .take(KEEP_LINES)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        write(&log_file, &kept)?;
    }
    Ok(())
}

pub(crate) fn clean_sweep() -> Result<()> {
    require_tool("cargo-sweep", "cargo install cargo-sweep")?;
    let root = workspace_root()?;
    if run_inherit("cargo", &["sweep", "--time", "7"], Some(&root))? != 0 {
        bail!("cargo sweep --time 7 failed");
    }
    Ok(())
}

pub(crate) fn clean_sweep_deps() -> Result<()> {
    require_tool("cargo-sweep", "cargo install cargo-sweep")?;
    let root = workspace_root()?;
    if run_inherit("cargo", &["sweep", "--installed"], Some(&root))? != 0 {
        bail!("cargo sweep --installed failed");
    }
    Ok(())
}

pub(crate) fn require_tool_cmd(args: RequireToolArgs) -> Result<()> {
    require_tool(&args.tool, &args.install_hint)
}

pub(crate) fn setup_sccache() -> Result<()> {
    if has_tool("sccache") {
        println!("sccache already installed");
    } else {
        println!("Installing sccache...");
        if run_inherit("cargo", &["install", "sccache", "--locked"], None)? != 0 {
            bail!("cargo install sccache --locked failed");
        }
    }
    Ok(())
}

pub(crate) fn setup_sccache_verify() -> Result<()> {
    println!("--- sccache 状态 ---");
    if has_tool("sccache") {
        let _ = run_inherit("sccache", &["--show-stats"], None)?;
    } else {
        println!("✗ sccache not in PATH");
    }

    println!("\n--- 环境变量检查 ---");
    match std::env::var("RUSTC_WRAPPER") {
        Ok(wrapper) if wrapper == "sccache" => println!("✓ RUSTC_WRAPPER=sccache"),
        Ok(wrapper) => println!("⚠ RUSTC_WRAPPER={wrapper} (应该是 sccache)"),
        Err(_) => println!("✗ RUSTC_WRAPPER 未设置"),
    }

    println!("\n--- .cargo/config.toml 检查 ---");
    let root = workspace_root()?;
    let config = root.join(".cargo/config.toml");
    if config.exists() && read(&config)?.contains("rustc-wrapper") {
        println!("✓ rustc-wrapper 已配置");
    } else {
        println!("✗ rustc-wrapper 未设置");
    }
    Ok(())
}

pub(crate) fn setup_hakari() -> Result<()> {
    if has_tool("cargo-hakari") {
        let version = run_capture("cargo", &["hakari", "--version"], None)
            .map(|outcome| outcome.output)
            .unwrap_or_else(|_| "unknown version".to_string());
        println!("cargo-hakari already installed: {version}");
    } else {
        println!("Installing cargo-hakari...");
        if run_inherit("cargo", &["install", "cargo-hakari"], None)? != 0 {
            bail!("cargo install cargo-hakari failed");
        }
    }
    println!("Generating unified dependency resolution...");
    if run_inherit("cargo", &["hakari", "generate"], None)? != 0 {
        bail!("cargo hakari generate failed");
    }
    if run_inherit("cargo", &["hakari", "manage-deps", "--yes"], None)? != 0 {
        bail!("cargo hakari manage-deps --yes failed");
    }
    println!("hakari configured — run cargo build to see speedup");
    Ok(())
}

pub(crate) fn setup_hakari_verify() -> Result<()> {
    let root = workspace_root()?;
    if root.join(".config/hakari.toml").exists() {
        println!("✓ .config/hakari.toml exists");
    } else {
        println!("✗ .config/hakari.toml missing");
    }
    if read(root.join("Cargo.toml"))?.contains("workspace-hack") {
        println!("✓ workspace-hack in members");
    } else {
        println!("✗ workspace-hack NOT in members");
    }
    Ok(())
}

pub(crate) fn setup_coverage() -> Result<()> {
    for (tool, install_name) in [
        ("cargo-llvm-cov", "cargo-llvm-cov"),
        ("cargo-sweep", "cargo-sweep"),
    ] {
        if !has_tool(tool) && run_inherit("cargo", &["install", install_name], None)? != 0 {
            bail!("cargo install {install_name} failed");
        }
    }
    if run_inherit("rustup", &["component", "add", "llvm-tools"], None)? != 0 {
        bail!("rustup component add llvm-tools failed");
    }
    println!("Coverage tools ready");
    Ok(())
}

pub(crate) fn audit_rust() -> Result<()> {
    if run_inherit("cargo", &["audit"], None)? != 0 {
        println!("Install cargo-audit: cargo install cargo-audit");
        bail!("cargo audit failed");
    }
    Ok(())
}

pub(crate) fn validate_unsafe_code() -> Result<()> {
    let root = workspace_root()?;
    let scan_roots = [
        "packages",
        "services",
        "servers",
        "workers",
        "platform/validators",
    ];
    let mut findings = Vec::new();

    for relative_root in scan_roots {
        let scan_root = root.join(relative_root);
        if !scan_root.exists() {
            continue;
        }

        for file in collect_files_with_extension(&scan_root, "rs") {
            let content = read(&file)?;
            if !contains_unsafe_code(&content) {
                continue;
            }
            let relative_path = file.strip_prefix(&root).unwrap_or(&file);
            findings.push(normalize_slashes(relative_path));
        }
    }

    println!("=== Unsafe Code Validation ===");
    println!("scope: production Rust crates");
    if findings.is_empty() {
        println!("PASS: no unsafe Rust markers found in production crate sources.");
        return Ok(());
    }

    println!("Findings:");
    findings.sort();
    findings.dedup();
    for finding in &findings {
        println!("  - {finding}");
    }
    bail!("unsafe Rust markers found in production crate sources")
}

fn contains_unsafe_code(content: &str) -> bool {
    let stripped = strip_rust_comments(content);
    stripped.contains("unsafe {")
        || stripped.contains("unsafe fn")
        || stripped.contains("unsafe impl")
        || stripped.contains("#[unsafe")
}

#[cfg(test)]
mod tests {
    use super::contains_unsafe_code;

    #[test]
    fn unsafe_marker_detection_ignores_comments() {
        let content = r#"
            // unsafe fn commented_out() {}
            /* unsafe { commented_out(); } */
            fn safe() {}
        "#;

        assert!(!contains_unsafe_code(content));
    }

    #[test]
    fn unsafe_marker_detection_finds_production_markers() {
        assert!(contains_unsafe_code("unsafe fn call() {}"));
        assert!(contains_unsafe_code("unsafe impl Send for Worker {}"));
        assert!(contains_unsafe_code("fn call() { unsafe { ffi(); } }"));
        assert!(contains_unsafe_code("#[unsafe(no_mangle)] fn entry() {}"));
    }
}

const K6_BASELINE_SCRIPT: &str = r#"import http from "k6/http";
import { check, sleep } from "k6";

export const options = {
  vus: __ENV.TEST_VUS ? parseInt(__ENV.TEST_VUS) : 5,
  duration: __ENV.TEST_DURATION || "30s",
  thresholds: {
    http_req_duration: ["p(95)<500"],
    http_req_failed: ["rate<0.65"],
    checks: ["rate>0.3"],
  },
};

const BASE_URL = __ENV.BASE_URL || "http://localhost:3010";

const B64_CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
function base64url(str) {
  var bytes = [];
  for (var i = 0; i < str.length; i++) {
    var c = str.charCodeAt(i);
    if (c < 0x80) bytes.push(c);
    else if (c < 0x800) { bytes.push(0xC0 | (c >> 6), 0x80 | (c & 0x3F)); }
    else { bytes.push(0xE0 | (c >> 12), 0x80 | ((c >> 6) & 0x3F), 0x80 | (c & 0x3F)); }
  }
  var result = "";
  for (var j = 0; j < bytes.length; j += 3) {
    var a = bytes[j], b = j + 1 < bytes.length ? bytes[j + 1] : 0, c = j + 2 < bytes.length ? bytes[j + 2] : 0;
    result += B64_CHARS[a >> 2];
    result += B64_CHARS[((a & 3) << 4) | (b >> 4)];
    result += j + 1 < bytes.length ? B64_CHARS[((b & 15) << 2) | (c >> 6)] : "=";
    result += j + 2 < bytes.length ? B64_CHARS[c & 63] : "=";
  }
  return result.replace(/=+$/, "").replace(/\+/g, "-").replace(/\//g, "_");
}

function uniqueUserSub() {
  return "k6-user-" + __VU + "-" + Date.now();
}

function devJwt(sub) {
  var header = base64url('{"alg":"HS256","typ":"JWT"}');
  var payload = base64url('{"sub":"' + sub + '"}');
  return header + "." + payload + ".dev-signature";
}

export default function () {
  var userSub = uniqueUserSub();
  var jwt = devJwt(userSub);
  var authHeaders = {
    "Content-Type": "application/json",
    Authorization: "Bearer " + jwt,
  };

  var healthRes = http.get(BASE_URL + "/healthz");
  check(healthRes, {
    "healthz 200": (r) => r.status === 200,
  });

  var tenantRes = http.post(
    BASE_URL + "/api/tenant/init",
    JSON.stringify({ user_sub: userSub, user_name: "k6-" + __VU }),
    { headers: authHeaders }
  );
  check(tenantRes, {
    "tenant init 200": (r) => r.status === 200,
  });

  for (var i = 0; i < 3; i++) {
    var incRes = http.post(
      BASE_URL + "/api/counter/increment",
      "{}",
      { headers: authHeaders }
    );
    check(incRes, {
      "increment 200": (r) => r.status === 200,
    });
  }

  var valRes = http.get(BASE_URL + "/api/counter/value", { headers: authHeaders });
  check(valRes, {
    "get value 200": (r) => r.status === 200,
    "value == 3": (r) => {
      try {
        return JSON.parse(r.body).value === 3;
      } catch {
        return false;
      }
    },
  });

  sleep(1);
}
"#;

pub(crate) fn k6_baseline(args: K6BaselineArgs) -> Result<()> {
    require_tool("k6", "install k6 via mise or your package manager")?;
    let root = workspace_root()?;
    let shell = Shell::new()?;
    shell.change_dir(&root);
    let base_url = args.base_url;
    let duration = args.duration;
    let vus = args.vus;

    let temp_dir = tempfile::tempdir()?;
    let script_path = temp_dir.path().join("k6-baseline.js");
    fs::write(&script_path, K6_BASELINE_SCRIPT)?;

    cmd!(
        shell,
        "k6 run -e BASE_URL={base_url} -e TEST_DURATION={duration} -e TEST_VUS={vus} {script_path}"
    )
    .run()?;
    Ok(())
}
