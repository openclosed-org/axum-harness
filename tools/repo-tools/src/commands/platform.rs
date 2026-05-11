use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use regex::Regex;

use crate::commands::secrets;
use crate::support::{
    Issue, Mode, Operation, OperationPhase, Report, collect_files_with_extension,
    copy_dir_contents, list_directories, normalize_slashes, read, run_capture, run_inherit,
    tempdir, workspace_root, write,
};

const DIRECTORY_CATEGORY_PATTERNS: &[(&str, &str)] = &[
    ("agent/", "agent"),
    (".agents/", "agent"),
    ("packages/contracts/", "contracts"),
    ("packages/core/", "shared"),
    ("packages/features/", "shared"),
    ("packages/shared/", "shared"),
    ("packages/sdk/typescript/", "contracts"),
    ("packages/web3/", "web3"),
    ("services/indexer", "web3"),
    ("tools/web3", "web3"),
    ("servers/", "backend"),
    ("services/", "backend"),
    ("packages/adapters/", "backend"),
    ("infra", "infra"),
    ("ops", "infra"),
    ("scripts", "infra"),
    ("justfiles", "infra"),
    (".cargo", "infra"),
    (".github", "infra"),
    (".config", "infra"),
    ("fixtures/", "tests"),
];

pub(crate) fn gen_directory_categories() -> Result<()> {
    let root = workspace_root()?;
    let directories = collect_directories(&root, 2)?;
    let category_map = categorize_directories(&directories);

    let mut categories = serde_json::Map::new();
    for (category, dirs) in &category_map {
        categories.insert(
            category.clone(),
            serde_json::Value::Array(
                dirs.iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }

    let output = serde_json::json!({
        "version": "2.0",
        "generated_at": chrono_like_timestamp(),
        "categories": categories,
        "phase": "2 — DDD Clean Architecture（业务逻辑在 services/*/ 中）",
        "priority_rules": [
            "始终阅读 'shared' 和 'contracts' 分类，无论任务类型。它们是系统的抽象基石。",
            "前端任务 → 优先搜索 'frontend' + 'shared' + 'contracts'。",
            "后端任务 → 优先搜索 'backend' + 'shared' + 'contracts'。业务逻辑在 services/<domain>/ 中，遵循 DDD Clean Architecture。",
            "全栈任务 → 平等对待 'frontend' + 'backend' + 'shared' + 'contracts'。",
            "基础设施任务 → 阅读 'infra' + 'agent'（约束定义）。",
            "测试文件在 'tests' 分类中 — 修改业务代码时不要忽略相关测试。",
            "新增业务模块时，应按目标架构在 services/<domain>/ 下创建 domain/application/ports/events/contracts/。"
        ]
    });

    let output_path = root.join("agent/directory_categories.json");
    write(&output_path, &serde_json::to_string_pretty(&output)?)?;

    println!(
        "Generated agent/directory_categories.json with {} categories.",
        category_map.len()
    );
    println!("   Directories scanned: {}", directories.len());
    for (category, dirs) in category_map {
        println!("   {category}: {} directories", dirs.len());
    }
    Ok(())
}

pub(crate) fn list_platform_inventory(kind: &str) -> Result<()> {
    let root = workspace_root()?;
    let dir = root.join("platform/model").join(kind);
    let title = match kind {
        "services" => "Services defined in platform model",
        "deployables" => "Deployables defined in platform model",
        "resources" => "Resources defined in platform model",
        "capabilities" => "Capabilities defined in platform model",
        _ => "Entries defined in platform model",
    };
    println!("{title}:");
    if !dir.is_dir() {
        println!(
            "  (none; missing {})",
            normalize_slashes(dir.strip_prefix(&root)?)
        );
        return Ok(());
    }
    let mut names = fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|ext| ext.to_str()) == Some("yaml"))
                .then(|| path.file_stem()?.to_str().map(ToOwned::to_owned))?
        })
        .collect::<Vec<_>>();
    names.sort();
    if names.is_empty() {
        println!("  (none)");
        return Ok(());
    }
    for name in names {
        println!("  - {name}");
    }
    Ok(())
}

pub(crate) fn clean_sdk() -> Result<()> {
    let root = workspace_root()?;
    println!("Cleaning generated SDKs...");
    let typescript_dir = root.join("packages/sdk/typescript");
    if typescript_dir.is_dir() {
        for entry in fs::read_dir(&typescript_dir)? {
            let entry = entry?;
            if entry.file_name() == ".gitkeep" {
                continue;
            }
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
    }
    println!("Skipping packages/sdk/rust cleanup because those files are tracked in-repo today");
    println!("SDKs cleaned");
    Ok(())
}

fn collect_directories(root: &Path, max_depth: usize) -> Result<Vec<String>> {
    fn walk(
        root: &Path,
        relative: &Path,
        depth: usize,
        max_depth: usize,
        out: &mut Vec<String>,
    ) -> Result<()> {
        if depth > max_depth {
            return Ok(());
        }
        let full_path = if relative.as_os_str().is_empty() {
            root.to_path_buf()
        } else {
            root.join(relative)
        };
        for entry in fs::read_dir(&full_path)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if should_skip_directory(&name) {
                continue;
            }
            let rel_path = relative.join(&name);
            out.push(normalize_slashes(&rel_path));
            walk(root, &rel_path, depth + 1, max_depth, out)?;
        }
        Ok(())
    }

    let mut dirs = Vec::new();
    walk(root, Path::new(""), 0, max_depth, &mut dirs)?;
    dirs.sort();
    Ok(dirs)
}

fn should_skip_directory(name: &str) -> bool {
    matches!(
        name,
        "node_modules" | "target" | ".jj" | ".moon" | ".cocoindex_code"
    ) || (name.starts_with('.') && !matches!(name, ".agents" | ".cargo" | ".config" | ".github"))
}

fn categorize_directories(dirs: &[String]) -> BTreeMap<String, BTreeSet<String>> {
    let mut categories: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let test_segment = Regex::new(r"(?:^|/)(?:tests?)(?:/|$)").expect("valid regex");
    for dir in dirs {
        if test_segment.is_match(dir) {
            categories
                .entry("tests".to_string())
                .or_default()
                .insert(dir.clone());
            continue;
        }
        if let Some((_, category)) = DIRECTORY_CATEGORY_PATTERNS
            .iter()
            .find(|(pattern, _)| dir.starts_with(*pattern))
        {
            categories
                .entry((*category).to_string())
                .or_default()
                .insert(dir.clone());
        }
    }
    categories
}

fn chrono_like_timestamp() -> String {
    let output = run_capture("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"], None);
    output
        .ok()
        .filter(|result| result.success && !result.output.is_empty())
        .map(|result| result.output)
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

pub(crate) fn validate_state(mode: Mode) -> Result<()> {
    let root = workspace_root()?;
    let services_dir = root.join("services");
    let ownership_map_path = root.join("platform/model/state/ownership-map.yaml");
    let mut ownership_map = BTreeMap::new();

    if ownership_map_path.exists() {
        let content = read(&ownership_map_path)?;
        for block in content.split("- entity:").skip(1) {
            let entity = Regex::new(r"^\s*([a-z][a-z0-9_-]*)")?
                .captures(block)
                .and_then(|caps| caps.get(1))
                .map(|m| m.as_str().to_string());
            let owner = Regex::new(r"(?m)^\s*owner_service:\s*([a-z][a-z0-9-]*)\s*$")?
                .captures(block)
                .and_then(|caps| caps.get(1))
                .map(|m| m.as_str().to_string());
            if let (Some(entity), Some(owner)) = (entity, owner) {
                ownership_map.insert(entity, owner);
            }
        }
    }

    let mut issues = Vec::new();
    let mut seen_entities = BTreeMap::new();
    for service_dir in list_directories(&services_dir)? {
        let model_path = service_dir.join("model.yaml");
        if !model_path.exists() {
            continue;
        }
        let content = read(&model_path)?;
        let service_dir_name = service_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        let service_name = Regex::new(r"(?m)^\s*name:\s*([a-z][a-z0-9-]*)\s*$")?
            .captures(&content)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| service_dir_name.clone());
        let owns_section = content
            .find("owns_entities:")
            .map(|start| &content[start..])
            .unwrap_or("");
        let entities: Vec<String> = Regex::new(r"(?m)^\s*-\s+name:\s*([a-z][a-z0-9_-]*)\s*$")?
            .captures_iter(owns_section)
            .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
            .collect();

        if entities.is_empty() {
            issues.push((
                mode.is_strict(),
                format!("services/{service_dir_name}/model.yaml"),
                "missing owns_entities declarations".to_string(),
            ));
            continue;
        }

        for entity in entities {
            if let Some(prior_owner) = seen_entities.get(&entity) {
                if prior_owner != &service_name {
                    issues.push((
                        true,
                        entity.clone(),
                        format!(
                            "entity declared by multiple services: {prior_owner} and {service_name}"
                        ),
                    ));
                }
            } else {
                seen_entities.insert(entity.clone(), service_name.clone());
            }

            match ownership_map.get(&entity) {
                None => issues.push((
                    mode.is_strict(),
                    entity.clone(),
                    format!("missing platform ownership-map entry for {service_name}"),
                )),
                Some(mapped_owner)
                    if mapped_owner != &service_name
                        && mapped_owner != &service_name.replace("-service", "") =>
                {
                    issues.push((true, entity.clone(), format!("ownership-map says {mapped_owner}, service semantics say {service_name}")));
                }
                _ => {}
            }
        }
    }

    let mut report = Report::new("validate-state", mode);
    report.extend(issues.iter().map(|(error, scope, message)| {
        if *error {
            Issue::error(scope.clone(), message.clone())
        } else {
            Issue::warn(scope.clone(), message.clone())
        }
    }));
    report.print();
    if issues.is_empty() {
        println!("State validation passed");
        return Ok(());
    }
    report.exit_if_needed();
    Ok(())
}

pub(crate) fn validate_workflows(mode: Mode) -> Result<()> {
    let root = workspace_root()?;
    let workflows_dir = root.join("platform/model/workflows");
    let mut issues = Vec::new();
    let required = [
        ("idempotency_key:", "missing top-level idempotency_key"),
        ("checkpoint_policy:", "missing checkpoint_policy"),
        ("compensation:", "missing compensation section"),
        ("recovery:", "missing recovery section"),
        ("resume_from:", "missing recovery resume strategy"),
        (
            "operator_intervention_on:",
            "missing operator intervention trigger",
        ),
    ];
    for file in collect_files_with_extension(&workflows_dir, "yaml") {
        let content = read(&file)?;
        let relative = normalize_slashes(file.strip_prefix(&root)?);
        for (snippet, message) in required {
            if !content.contains(snippet) {
                issues.push((mode.is_strict(), relative.clone(), message.to_string()));
            }
        }
        for block in content.split("\n- name:").skip(1) {
            let step_name = block
                .lines()
                .next()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .unwrap_or("unknown-step");
            if !block.contains("checkpoint: true") {
                issues.push((
                    mode.is_strict(),
                    relative.clone(),
                    format!("step {step_name} is missing checkpoint: true"),
                ));
            }
            if !block.contains("idempotency_key:") {
                issues.push((
                    mode.is_strict(),
                    relative.clone(),
                    format!("step {step_name} is missing idempotency_key"),
                ));
            }
        }
    }

    let mut report = Report::new("validate-workflows", mode);
    report.extend(issues.iter().map(|(error, scope, message)| {
        if *error {
            Issue::error(scope.clone(), message.clone())
        } else {
            Issue::warn(scope.clone(), message.clone())
        }
    }));
    report.print();
    if issues.is_empty() {
        println!("Workflow validation passed");
        return Ok(());
    }
    report.exit_if_needed();
    Ok(())
}
pub(crate) fn verify_counter_delivery(mode: Mode) -> Result<()> {
    let operation = Operation::new("verify-counter-delivery", false);
    let root = workspace_root()?;
    operation.phase(
        OperationPhase::Plan,
        "check delivery artifacts, overlays, runbooks, deployables, and secrets",
    );
    let mut failures = Vec::new();
    for relative_path in [
        "infra/security/sops/dev/counter-db-credentials.enc.yaml",
        "infra/k3s/overlays/dev/outbox-relay-worker/kustomization.yaml",
        "infra/k3s/overlays/dev/projector-worker/kustomization.yaml",
        "infra/k3s/overlays/staging/outbox-relay-worker/kustomization.yaml",
        "infra/k3s/overlays/staging/projector-worker/kustomization.yaml",
        "infra/gitops/flux/apps/staging-outbox-relay-worker.yaml",
        "infra/gitops/flux/apps/staging-projector-worker.yaml",
        "infra/gitops/flux/apps/outbox-relay-worker.yaml",
        "infra/gitops/flux/apps/projector-worker.yaml",
        "docs/operations/counter-service-reference-chain.md",
        "ops/runbooks/counter-delivery.md",
        "platform/model/deployables/web-bff.yaml",
        "platform/model/deployables/outbox-relay-worker.yaml",
        "platform/model/deployables/projector-worker.yaml",
        "platform/model/deployables/counter-service.yaml",
        "platform/model/services/counter-service.yaml",
        "platform/model/state/ownership-map.yaml",
    ] {
        if !root.join(relative_path).exists() {
            failures.push(format!(
                "{relative_path}: required counter delivery artifact missing"
            ));
        }
    }
    for (relative_path, pattern, reason) in [
        (
            "infra/k3s/overlays/dev/outbox-relay-worker/kustomization.yaml",
            r"counter-db-credentials",
            "outbox relay overlay must consume counter DB credentials secret",
        ),
        (
            "infra/k3s/overlays/dev/projector-worker/kustomization.yaml",
            r"counter-db-credentials",
            "projector overlay must consume counter DB credentials secret",
        ),
        (
            "infra/k3s/overlays/dev/outbox-relay-worker/kustomization.yaml",
            r"path:\s*/spec/replicas[\s\S]*value:\s*1",
            "outbox relay overlay must keep replicas patched to 1",
        ),
        (
            "infra/k3s/overlays/dev/projector-worker/kustomization.yaml",
            r"path:\s*/spec/replicas[\s\S]*value:\s*1",
            "projector overlay must keep replicas patched to 1",
        ),
        (
            "infra/gitops/flux/apps/outbox-relay-worker.yaml",
            r"path:\s*\./infra/k3s/overlays/dev/outbox-relay-worker",
            "outbox relay Flux app must point at its dev overlay",
        ),
        (
            "infra/gitops/flux/apps/projector-worker.yaml",
            r"path:\s*\./infra/k3s/overlays/dev/projector-worker",
            "projector Flux app must point at its dev overlay",
        ),
        (
            "infra/gitops/flux/apps/staging-outbox-relay-worker.yaml",
            r"path:\s*\./infra/k3s/overlays/staging/outbox-relay-worker",
            "staging outbox relay Flux app must point at its staging overlay",
        ),
        (
            "infra/gitops/flux/apps/staging-projector-worker.yaml",
            r"path:\s*\./infra/k3s/overlays/staging/projector-worker",
            "staging projector Flux app must point at its staging overlay",
        ),
        (
            "infra/gitops/flux/apps/staging-outbox-relay-worker.yaml",
            r"ENV:\s*staging",
            "staging outbox relay Flux app must substitute ENV=staging",
        ),
        (
            "infra/k3s/overlays/staging/outbox-relay-worker/kustomization.yaml",
            r"counter-db-credentials-staging",
            "staging outbox relay overlay must reference staging counter DB secret",
        ),
        (
            "docs/operations/counter-service-reference-chain.md",
            r"verify-counter-delivery|counter delivery gate",
            "counter reference chain must mention executable delivery admission",
        ),
        (
            "ops/runbooks/counter-delivery.md",
            r"just verify-counter-delivery strict|counter-db-credentials",
            "counter delivery runbook must document executable admission and counter DB credentials checks",
        ),
        (
            "platform/model/state/ownership-map.yaml",
            r"entity:\s*counter",
            "ownership-map must declare counter entity owner",
        ),
        (
            "platform/model/deployables/web-bff.yaml",
            r"counter-service",
            "web-bff deployable must list counter-service",
        ),
        (
            "platform/model/deployables/counter-service.yaml",
            r"current_status:\s*embedded[\s\S]*target_status:\s*independent[\s\S]*embedded_in:\s*\n\s*-\s*web-bff",
            "counter-service deployable must distinguish current embedded status from independent target status",
        ),
        (
            "platform/model/deployables/projector-worker.yaml",
            r"runtime_profile:\s*async-projection",
            "projector-worker deployable must declare async-projection profile",
        ),
        (
            "ops/runbooks/counter-delivery.md",
            r"rollback",
            "counter delivery runbook must document rollback procedure",
        ),
    ] {
        let full_path = root.join(relative_path);
        if !full_path.exists() {
            failures.push(format!("{relative_path}: missing file"));
            continue;
        }
        if !Regex::new(pattern)?.is_match(&read(&full_path)?) {
            failures.push(format!("{relative_path}: {reason}"));
        }
    }
    if let Err(error) = secrets::verify_counter_db_credentials("dev") {
        failures.push(format!(
            "counter DB credentials secret verification failed: {error}"
        ));
    }
    operation.phase(OperationPhase::Verify, "delivery evidence checks finished");
    if failures.is_empty() {
        println!("Counter delivery verification passed (promotion + drift + rollback)");
        return Ok(());
    }
    let mut report = Report::new("verify-counter-delivery", mode);
    report.extend(
        failures
            .iter()
            .map(|failure| Issue::warn("counter-delivery", failure.clone())),
    );
    report.print();
    report.exit_if_needed();
    Ok(())
}

pub(crate) fn verify_generated_artifacts() -> Result<()> {
    let root = workspace_root()?;
    println!("=== Verifying generated artifacts ===");
    let temp_root = tempdir()?;
    let mut overall_drift = false;
    check_directory_with_regen(
        &root,
        temp_root.path(),
        &mut overall_drift,
        "platform",
        "verification/golden/generated-platform",
        &[
            "run",
            "-p",
            "platform-generator",
            "--quiet",
            "--",
            "--platform-dir",
            "platform",
            "--output-dir",
        ],
    )?;
    check_directory(
        &root,
        &mut overall_drift,
        "contracts",
        "packages/contracts/generated",
        "verification/golden/contracts",
    )?;
    check_directory(
        &root,
        &mut overall_drift,
        "sdk-typescript",
        "packages/sdk/typescript",
        "verification/golden/sdk-typescript",
    )?;
    check_directory(
        &root,
        &mut overall_drift,
        "sdk-rust",
        "packages/sdk/rust",
        "verification/golden/sdk-rust",
    )?;
    println!("\n=============================================================");
    if !overall_drift {
        println!("  No drift detected — all generated artifacts match golden baselines");
        println!("=============================================================");
        return Ok(());
    }
    println!("  DRIFT DETECTED — some generated artifacts differ from baselines");
    println!();
    println!("  To regenerate and update golden baselines, run:");
    println!("    just commit-golden-baseline");
    println!("=============================================================");
    bail!("generated artifact drift detected")
}

pub(crate) fn commit_golden_baseline() -> Result<()> {
    let operation = Operation::new("commit-golden-baseline", true);
    let root = workspace_root()?;
    let golden_root = root.join("verification/golden");
    operation.phase(
        OperationPhase::Plan,
        format!("update golden baselines under {}", golden_root.display()),
    );
    println!("=== Committing golden baselines for all generated artifacts ===\n");
    println!("1. Generating platform catalog...");
    operation.phase(OperationPhase::Execute, "generate platform catalog");
    if run_inherit(
        "cargo",
        &[
            "run",
            "-p",
            "platform-generator",
            "--quiet",
            "--",
            "--platform-dir",
            "platform",
        ],
        Some(&root),
    )? != 0
    {
        bail!("platform catalog generation failed");
    }
    println!("  Platform catalog generated\n");
    println!("2. Copying platform catalog to golden baseline...");
    copy_to_golden(
        &root.join("platform/catalog"),
        &golden_root.join("generated-platform"),
        "generated-platform",
    )?;
    println!();
    println!("3. Generating contract bindings...");
    operation.phase(OperationPhase::Execute, "generate contract bindings");
    if run_inherit(
        "cargo",
        &["run", "-p", "repo-tools", "--", "typegen"],
        Some(&root),
    )? != 0
    {
        bail!("typegen failed");
    }
    println!("  Contract bindings generated\n");
    println!("4. Copying contract bindings to golden baseline...");
    copy_to_golden(
        &root.join("packages/contracts/generated"),
        &golden_root.join("contracts"),
        "contracts",
    )?;
    println!();
    let just_list = run_capture("just", &["--list"], Some(&root))?;
    let has_gen_sdk = just_list.success && just_list.output.contains("gen-sdk");
    println!("5. Checking TypeScript SDK...");
    if has_gen_sdk {
        println!("  Running gen-sdk...");
        if run_inherit("just", &["gen-sdk"], Some(&root))? != 0 {
            bail!("gen-sdk failed");
        }
        copy_to_golden(
            &root.join("packages/sdk/typescript"),
            &golden_root.join("sdk-typescript"),
            "sdk-typescript",
        )?;
    } else {
        println!("  SKIP: gen-sdk command not yet defined");
        fs::create_dir_all(golden_root.join("sdk-typescript"))?;
    }
    println!();
    println!("6. Checking Rust SDK...");
    if has_gen_sdk {
        copy_to_golden(
            &root.join("packages/sdk/rust"),
            &golden_root.join("sdk-rust"),
            "sdk-rust",
        )?;
    } else {
        println!("  SKIP: gen-sdk command not yet defined");
        fs::create_dir_all(golden_root.join("sdk-rust"))?;
    }
    println!();
    println!("=============================================================");
    operation.phase(OperationPhase::Verify, "golden baseline update completed");
    println!("  Golden baselines updated:");
    for dir in [
        "generated-platform",
        "contracts",
        "sdk-typescript",
        "sdk-rust",
    ] {
        let target = golden_root.join(dir);
        if target.exists() {
            println!("    - {dir}/ ({} files)", count_non_gitkeep_files(&target));
        }
    }
    println!();
    println!(
        "  Next step: git add {} && git commit -m 'chore: update golden baselines'",
        golden_root.display()
    );
    println!("=============================================================");
    Ok(())
}
fn count_non_gitkeep_files(dir: &Path) -> usize {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && entry.file_name() != ".gitkeep")
        .count()
}

fn compare_tree(source_dir: &Path, golden_dir: &Path) -> Result<bool> {
    let mut drift = false;
    for entry in walkdir::WalkDir::new(golden_dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() || entry.file_name() == ".gitkeep" {
            continue;
        }
        let relative = entry.path().strip_prefix(golden_dir)?;
        let source_file = source_dir.join(relative);
        if !source_file.is_file() {
            println!(
                "  DRIFT: Missing generated file: {}",
                normalize_slashes(relative)
            );
            drift = true;
            continue;
        }
        if fs::read(entry.path())? != fs::read(&source_file)? {
            println!(
                "  DRIFT: {} differs from baseline",
                normalize_slashes(relative)
            );
            drift = true;
        } else {
            println!("  OK: {}", normalize_slashes(relative));
        }
    }
    for entry in walkdir::WalkDir::new(source_dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() || entry.file_name() == ".gitkeep" {
            continue;
        }
        let relative = entry.path().strip_prefix(source_dir)?;
        if !golden_dir.join(relative).is_file() {
            println!(
                "  DRIFT: Extra generated file (not in baseline): {}",
                normalize_slashes(relative)
            );
            drift = true;
        }
    }
    Ok(drift)
}

fn check_directory(
    root: &Path,
    overall_drift: &mut bool,
    name: &str,
    source_dir: &str,
    golden_dir: &str,
) -> Result<()> {
    println!("\n--- Checking: {name} ---");
    let source = root.join(source_dir);
    let golden = root.join(golden_dir);
    if !source.is_dir() {
        println!("  SKIP: Source directory not found: {}", source.display());
        return Ok(());
    }
    if !golden.is_dir() {
        println!("  SKIP: Golden baseline not found: {}", golden.display());
        println!("  Run 'just commit-golden-baseline' to create it.");
        return Ok(());
    }
    if count_non_gitkeep_files(&golden) == 0 {
        println!("  SKIP: Golden baseline is empty (no files to compare)");
        println!("  Run 'just commit-golden-baseline' after generating artifacts.");
        return Ok(());
    }
    if count_non_gitkeep_files(&source) == 0 {
        println!("  SKIP: Source directory is empty (only .gitkeep)");
        println!("  Run the appropriate generation command first.");
        return Ok(());
    }
    *overall_drift |= compare_tree(&source, &golden)?;
    Ok(())
}

fn check_directory_with_regen(
    root: &Path,
    temp_root: &Path,
    overall_drift: &mut bool,
    name: &str,
    golden_dir: &str,
    regen_prefix: &[&str],
) -> Result<()> {
    println!("\n--- Checking: {name} (with regeneration) ---");
    let golden = root.join(golden_dir);
    if !golden.is_dir() {
        println!("  SKIP: Golden baseline not found: {}", golden.display());
        println!("  Run 'just commit-golden-baseline' to create it.");
        return Ok(());
    }
    if count_non_gitkeep_files(&golden) == 0 {
        println!("  SKIP: Golden baseline is empty (no files to compare)");
        return Ok(());
    }
    let temp_dir = temp_root.join(name);
    fs::create_dir_all(&temp_dir)?;
    println!("  Regenerating {name}...");
    let mut args = regen_prefix
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    args.push(temp_dir.display().to_string());
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    if run_inherit("cargo", &refs, Some(root))? != 0 {
        println!("  ERROR: Regeneration failed for {name}");
        println!("  Skipping drift check (cannot compare without fresh generation)");
        return Ok(());
    }
    *overall_drift |= compare_tree(&temp_dir, &golden)?;
    Ok(())
}

fn copy_to_golden(source_dir: &Path, golden_dir: &Path, name: &str) -> Result<()> {
    fs::create_dir_all(golden_dir)?;
    if count_non_gitkeep_files(source_dir) == 0 {
        println!("  {name}: SKIP (no files to copy)");
        return Ok(());
    }
    for entry in walkdir::WalkDir::new(golden_dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() && entry.file_name() != ".gitkeep" {
            fs::remove_file(entry.path())?;
        }
    }
    copy_dir_contents(source_dir, golden_dir)?;
    println!("  {name}: {} files", count_non_gitkeep_files(golden_dir));
    Ok(())
}
