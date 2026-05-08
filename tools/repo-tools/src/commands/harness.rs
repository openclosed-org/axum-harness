use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};

use crate::cli::{GateArgs, GateGuidanceArgs, GateName, RouteTaskArgs, VerifyHandoffArgs};
use crate::core::manifest::{cargo_metadata, load_codemap, load_gate_matrix, load_routing_rules};
use crate::support::{
    Issue, Mode, Report, collect_files_named, except_path, git_changed_paths, normalize_slashes,
    pattern_matches, read, run_capture, same_module, workspace_root,
};

#[derive(Clone)]
struct GateCommand {
    label: &'static str,
    program: &'static str,
    args: Vec<String>,
}

pub(crate) fn gate_guidance(args: GateGuidanceArgs) -> Result<()> {
    let root = workspace_root()?;
    let routing = load_routing_rules(&root)?;
    let gate_matrix = load_gate_matrix(&root)?;
    let mut legacy_agents = routing
        .rules
        .iter()
        .map(|rule| rule.primary.as_str())
        .filter(|agent| *agent != "planner")
        .collect::<Vec<_>>();
    legacy_agents.sort();
    legacy_agents.dedup();

    if args.list {
        println!("\n=== Gate Selection ===\n");
        println!("Gate selection is path/risk/evidence based, not subagent based.");
        println!(
            "Use agent/manifests/gate-matrix.yml to select advisory, guardrail, or invariant gates."
        );
        println!("Default backend-core guardrail: just check-backend-primary");
        println!("Broader repo-wide guardrail when needed: just verify");
        println!("Release/P0 invariant gate only when justified: just gate-release");
        println!(
            "Loaded {} path rule(s) from agent/manifests/gate-matrix.yml.",
            gate_matrix.path_rules.len()
        );
        println!("\nAccepted agent scopes for this helper:");
        for agent in &legacy_agents {
            println!("  - {agent}");
        }
        return Ok(());
    }

    let agent = args.agent.context("missing agent scope")?;
    if !legacy_agents.contains(&agent.as_str()) {
        bail!("unknown agent scope: {agent}");
    }

    println!("\n=== Gate Selection for {agent} ===\n");
    println!("No gate is required solely because this subagent handled the change.");
    println!(
        "Select gates from changed paths, risk, and evidence level in agent/manifests/gate-matrix.yml."
    );
    let agent_patterns = routing
        .rules
        .iter()
        .filter(|rule| rule.primary == agent)
        .map(|rule| rule.r#match.as_str())
        .collect::<Vec<_>>();
    let path_rule_count = gate_matrix
        .path_rules
        .iter()
        .filter(|rule| {
            rule.r#match.iter().any(|pattern| {
                agent_patterns.iter().any(|agent_pattern| {
                    pattern_matches(pattern, agent_pattern)
                        || pattern_matches(agent_pattern, pattern)
                })
            })
        })
        .count();
    println!("Relevant path rules loaded: {path_rule_count}");
    println!("This compatibility helper does not run heavy gates automatically.");
    Ok(())
}

pub(crate) fn route_task(args: RouteTaskArgs) -> Result<()> {
    let root = workspace_root()?;
    let routing = load_routing_rules(&root)?;
    let dispatch_order = routing
        .dispatch_order
        .iter()
        .filter(|agent| agent.as_str() != "(verify)")
        .cloned()
        .collect::<Vec<_>>();

    if args.list {
        println!("\n=== Routing Rules ===\n");
        println!("Path Pattern -> Subagent\n");
        for rule in &routing.rules {
            println!("  {:<35} -> {}", rule.r#match, rule.primary);
        }
        println!(
            "\nDispatch order: {} -> (verify)",
            dispatch_order.join(" -> ")
        );
        return Ok(());
    }

    let paths = if !args.paths.is_empty() {
        args.paths
    } else if let Some(diff_range) = args.diff {
        let result = run_capture("git", &["diff", "--name-only", &diff_range], Some(&root))?;
        result.output.lines().map(ToOwned::to_owned).collect()
    } else {
        git_changed_paths(&root)?
    };

    if paths.is_empty() {
        println!("No files to analyze. Stage changes or specify paths.");
        return Ok(());
    }

    let mut by_agent: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for path in &paths {
        if let Some(rule) = routing
            .rules
            .iter()
            .find(|rule| pattern_matches(path, &rule.r#match))
        {
            by_agent
                .entry(rule.primary.as_str())
                .or_default()
                .push(path.clone());
        }
    }

    println!("\n=== Task Routing Result ===\n");
    if by_agent.is_empty() {
        println!("No subagent domains affected by touched paths.");
        println!("Planner can handle this directly.");
        for path in paths {
            println!("  {path}");
        }
        return Ok(());
    }

    let affected: Vec<&str> = dispatch_order
        .iter()
        .map(String::as_str)
        .filter(|agent| by_agent.contains_key(agent))
        .collect();
    let planner_paths = by_agent.get("planner").cloned().unwrap_or_default();
    let mut dispatch = affected.clone();
    if !planner_paths.is_empty() {
        dispatch.insert(0, "planner");
    }
    dispatch.push("(verify)");
    let mut affected_domains = affected.iter().copied().collect::<Vec<_>>();
    if !planner_paths.is_empty() {
        affected_domains.insert(0, "planner");
    }
    println!("Affected domains:    {}", affected_domains.join(", "));
    println!("Dispatch order:      {}", dispatch.join(" -> "));
    println!("\nPath -> Agent mapping:");
    if !planner_paths.is_empty() {
        println!("\n  planner:");
        for path in &planner_paths {
            println!("    {path}");
        }
    }
    for agent in affected {
        println!("\n  {agent}:");
        if let Some(agent_paths) = by_agent.get(agent) {
            for path in agent_paths {
                println!("    {path}");
            }
        }
    }
    Ok(())
}
pub(crate) fn gate(args: GateArgs) -> Result<()> {
    let mode = args.mode().unwrap_or(match args.gate {
        GateName::Ci | GateName::Release => Mode::Strict,
        GateName::Local | GateName::Prepush => Mode::Warn,
    });
    let release_type = std::env::var("RELEASE_TYPE").unwrap_or_else(|_| "minor".to_string());
    let commands = match args.gate {
        GateName::Local => vec![
            GateCommand {
                label: "toolchain doctor",
                program: "just",
                args: vec!["doctor".into()],
            },
            GateCommand {
                label: "format check",
                program: "just",
                args: vec!["fmt".into()],
            },
            GateCommand {
                label: "lint",
                program: "just",
                args: vec!["lint".into()],
            },
        ],
        GateName::Prepush => vec![
            GateCommand {
                label: "existence validation",
                program: "just",
                args: vec!["gate-existence".into(), "warn".into()],
            },
            GateCommand {
                label: "import validation",
                program: "just",
                args: vec!["gate-imports".into(), "strict".into()],
            },
            GateCommand {
                label: "publish intent validation",
                program: "just",
                args: vec!["validate-publish-intent".into(), "strict".into()],
            },
            GateCommand {
                label: "typecheck",
                program: "just",
                args: vec!["typecheck".into()],
            },
            GateCommand {
                label: "unit test",
                program: "just",
                args: vec!["test".into()],
            },
            GateCommand {
                label: "platform validation",
                program: "just",
                args: vec!["validate-platform".into()],
            },
        ],
        GateName::Ci => vec![
            GateCommand {
                label: "full verify",
                program: "just",
                args: vec!["verify".into()],
            },
            GateCommand {
                label: "platform doctor",
                program: "just",
                args: vec!["platform-doctor".into()],
            },
            GateCommand {
                label: "validate state",
                program: "cargo",
                args: vec![
                    "run".into(),
                    "-p".into(),
                    "repo-tools".into(),
                    "--".into(),
                    "validate-state".into(),
                    "--mode".into(),
                    "strict".into(),
                ],
            },
            GateCommand {
                label: "boundary check",
                program: "cargo",
                args: vec![
                    "run".into(),
                    "-p".into(),
                    "repo-tools".into(),
                    "--".into(),
                    "boundary-check".into(),
                ],
            },
            GateCommand {
                label: "publish intent validation",
                program: "cargo",
                args: vec![
                    "run".into(),
                    "-p".into(),
                    "repo-tools".into(),
                    "--".into(),
                    "validate-publish-intent".into(),
                    "--mode".into(),
                    "strict".into(),
                ],
            },
        ],
        GateName::Release => vec![
            GateCommand {
                label: "semver compatibility",
                program: "just",
                args: vec!["semver-check".into(), "".into(), release_type],
            },
            GateCommand {
                label: "contract drift",
                program: "just",
                args: vec!["drift-check".into()],
            },
            GateCommand {
                label: "backend-core app-shell audit",
                program: "just",
                args: vec!["audit-backend-core".into(), "strict".into()],
            },
            GateCommand {
                label: "ci gate",
                program: "cargo",
                args: vec![
                    "run".into(),
                    "-p".into(),
                    "repo-tools".into(),
                    "--".into(),
                    "gate".into(),
                    "ci".into(),
                    "--mode".into(),
                    "strict".into(),
                ],
            },
            GateCommand {
                label: "release build",
                program: "just",
                args: vec!["build-release".into()],
            },
        ],
    };

    println!(
        "=== gate-{} ({}) ===",
        gate_name_label(args.gate),
        if mode.is_strict() { "strict" } else { "warn" }
    );
    let mut failures = Vec::new();
    for step in commands {
        println!(
            "\n-> {}: {} {}",
            step.label,
            step.program,
            step.args.join(" ")
        );
        let refs: Vec<&str> = step.args.iter().map(String::as_str).collect();
        let outcome = run_capture(step.program, &refs, None)?;
        if !outcome.output.is_empty() {
            println!("{}", outcome.output);
        }
        if outcome.success {
            println!("✓ {}", step.label);
            continue;
        }
        if !outcome.error.is_empty() {
            eprintln!("{}", outcome.error);
        }
        failures.push((step.label, outcome.exit_code));
        eprintln!(
            "✗ {} failed with exit code {}",
            step.label, outcome.exit_code
        );
        if mode.is_strict() {
            bail!(
                "gate-{} blocked by {}",
                gate_name_label(args.gate),
                step.label
            );
        }
    }

    if failures.is_empty() {
        println!("\n✓ gate-{} passed", gate_name_label(args.gate));
        return Ok(());
    }

    println!(
        "\n! gate-{} completed with {} warning(s)",
        gate_name_label(args.gate),
        failures.len()
    );
    for (label, exit_code) in failures {
        println!("  - {label}: exit {exit_code}");
    }
    Ok(())
}

pub(crate) fn gate_name_label(gate: GateName) -> &'static str {
    match gate {
        GateName::Local => "local",
        GateName::Prepush => "prepush",
        GateName::Ci => "ci",
        GateName::Release => "release",
    }
}

struct SubagentBoundary {
    writable: Vec<String>,
    readonly: Vec<String>,
}

pub(crate) fn verify_handoff(args: VerifyHandoffArgs) -> Result<()> {
    let root = workspace_root()?;
    let boundaries = subagent_boundaries(&root)?;
    let Some(boundary) = boundaries.get(args.agent.as_str()) else {
        eprintln!("Unknown subagent: {}", args.agent);
        eprintln!("Available subagents:");
        for name in boundaries.keys() {
            eprintln!("  {name}");
        }
        bail!("unknown subagent: {}", args.agent);
    };

    println!("\n=== Verifying handoff for {} ===\n", args.agent);
    let paths = modified_paths()?;

    if paths.is_empty() {
        println!("No modified files to verify.");
        println!("Print gate-selection guidance anyway...");
    }

    println!("Modified files: {}", paths.len());
    for path in &paths {
        println!("  {path}");
    }

    println!("\n--- Boundary Check ---");
    let mut valid = Vec::new();
    let mut violations = Vec::new();
    for path in &paths {
        let in_writable = boundary
            .writable
            .iter()
            .any(|item| path_matches_boundary(path, item));
        let in_readonly = boundary
            .readonly
            .iter()
            .any(|item| path_matches_boundary(path, item));
        if in_writable || !in_readonly {
            valid.push(path.clone());
        } else {
            violations.push(format!(
                "{path} (read-only - generated or owned by another agent)"
            ));
        }
    }

    if !violations.is_empty() {
        eprintln!("\nBoundary violations:");
        for violation in violations {
            eprintln!("  {violation}");
        }
        bail!("handoff blocked by boundary violations");
    }

    println!(
        "All {} modified files are within writable boundaries",
        valid.len()
    );

    if args.agent == "app-shell-agent" {
        println!("\nNo root-scoped verification is defined for app-shell-agent.");
        println!(
            "Validate retained app shells from their own local command surface if those directories remain in the repo."
        );
        println!("\n=== Handoff Verified ===");
        println!("{} changes are ready for convergence.", args.agent);
        println!("Next step: select gates by changed paths, risk, and evidence level.");
        return Ok(());
    }

    println!("\n--- Gate Selection Guidance ---");
    gate_guidance(GateGuidanceArgs {
        list: false,
        agent: Some(args.agent.clone()),
    })?;

    println!("\n=== Handoff Verified ===");
    println!("{} changes are ready for convergence.", args.agent);
    println!("Next step: run gates selected by changed paths, risk, and evidence level.");
    Ok(())
}

fn subagent_boundaries(root: &std::path::Path) -> Result<BTreeMap<String, SubagentBoundary>> {
    let codemap = load_codemap(root)?;
    let mut boundaries = BTreeMap::new();
    for (agent, boundary) in codemap.write_boundaries {
        boundaries.insert(
            agent,
            SubagentBoundary {
                writable: boundary.may_modify,
                readonly: boundary.must_not_modify,
            },
        );
    }
    Ok(boundaries)
}

fn modified_paths() -> Result<Vec<String>> {
    let root = workspace_root()?;
    let staged = run_capture(
        "git",
        &["diff", "--staged", "--name-only", "--diff-filter=ACMR"],
        Some(&root),
    )?;
    if staged.success && !staged.output.trim().is_empty() {
        return Ok(staged.output.lines().map(ToOwned::to_owned).collect());
    }

    let unstaged = run_capture("git", &["diff", "--name-only"], Some(&root))?;
    if unstaged.success && !unstaged.output.trim().is_empty() {
        return Ok(unstaged.output.lines().map(ToOwned::to_owned).collect());
    }

    Ok(Vec::new())
}

fn path_matches_boundary(path: &str, boundary: &str) -> bool {
    path.starts_with(boundary) || path.contains(&format!("/{boundary}"))
}

pub(crate) fn validate_existence(mode: Mode) -> Result<()> {
    let root = workspace_root()?;
    let codemap: serde_yaml::Value = serde_yaml::from_str(&read(root.join("agent/codemap.yml"))?)?;
    let required_files = codemap
        .get("rules")
        .and_then(|value| value.get("required_files"))
        .and_then(serde_yaml::Value::as_mapping);

    let mut declared = BTreeMap::new();
    for section in ["modules", "reference_modules"] {
        let Some(mapping) = codemap.get(section).and_then(serde_yaml::Value::as_mapping) else {
            continue;
        };
        for (group, items) in mapping {
            let Some(group) = group.as_str() else {
                continue;
            };
            let kind = group.trim_end_matches('s');
            if !matches!(kind, "service" | "server" | "worker") {
                continue;
            }
            let Some(items) = items.as_mapping() else {
                continue;
            };
            for item in items.values() {
                let Some(item) = item.as_mapping() else {
                    continue;
                };
                let path = item
                    .get(serde_yaml::Value::from("path"))
                    .and_then(serde_yaml::Value::as_str);
                let notes = item
                    .get(serde_yaml::Value::from("notes"))
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or_default();
                let status = item
                    .get(serde_yaml::Value::from("status"))
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or_default();
                if let Some(path) = path {
                    declared.insert(
                        path.to_string(),
                        (kind.to_string(), notes.to_string(), status.to_string()),
                    );
                }
            }
        }
    }

    let mut issues = Vec::new();
    for (module_path, (kind, notes, status)) in &declared {
        if *status == "planned"
            || notes.contains("尚未实现")
            || notes.contains("占位")
            || notes.contains("仅保留语义边界")
        {
            continue;
        }
        let absolute = root.join(module_path);
        if !absolute.exists() {
            issues.push((
                mode.is_strict(),
                module_path.clone(),
                "declared in codemap but directory does not exist".to_string(),
            ));
            continue;
        }
        let entries = required_files
            .and_then(|mapping| {
                mapping
                    .get(serde_yaml::Value::from(kind.as_str()))
                    .and_then(serde_yaml::Value::as_sequence)
            })
            .cloned()
            .unwrap_or_default();
        for entry in entries.iter().filter_map(serde_yaml::Value::as_str) {
            let required_path = if entry.contains("<name>") {
                let module_name = absolute
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                absolute.join(entry.replace("<name>", module_name))
            } else {
                absolute.join(entry)
            };
            if !required_path.exists() {
                issues.push((
                    mode.is_strict(),
                    module_path.clone(),
                    format!("missing required path: {entry}"),
                ));
            }
        }
    }

    for kind in ["service", "server", "worker"] {
        let base = root.join(format!("{kind}s"));
        if !base.exists() {
            continue;
        }
        for manifest in collect_files_named(&base, "Cargo.toml") {
            let Some(module_dir) = manifest.parent() else {
                continue;
            };
            let relative = normalize_slashes(module_dir.strip_prefix(&root)?);
            if !declared.contains_key(&relative) {
                issues.push((
                    false,
                    relative,
                    "exists in repository but is not declared in agent/codemap.yml".to_string(),
                ));
            }
        }
    }

    let mut report = Report::new("validate-existence", mode);
    report.extend(issues.iter().map(|(error, scope, message)| {
        if *error {
            Issue::error(scope.clone(), message.clone())
        } else {
            Issue::warn(scope.clone(), message.clone())
        }
    }));
    report.print();
    if issues.is_empty() {
        println!("No existence issues found");
        return Ok(());
    }
    report.exit_if_needed();
    Ok(())
}

pub(crate) fn validate_imports(mode: Mode) -> Result<()> {
    let root = workspace_root()?;
    let codemap: serde_yaml::Value = serde_yaml::from_str(&read(root.join("agent/codemap.yml"))?)?;
    let workspace_toml: toml::Value = toml::from_str(&read(root.join("Cargo.toml"))?)?;
    let workspace_paths = workspace_toml
        .get("workspace")
        .and_then(|value| value.get("dependencies"))
        .and_then(toml::Value::as_table)
        .map(|table| {
            table
                .iter()
                .filter_map(|(name, value)| {
                    value
                        .get("path")
                        .and_then(toml::Value::as_str)
                        .map(|path| (name.clone(), path.replace('\\', "/")))
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let rules = codemap
        .get("rules")
        .and_then(|value| value.get("imports"))
        .and_then(serde_yaml::Value::as_sequence)
        .cloned()
        .unwrap_or_default();

    let mut issues = Vec::new();
    for scope in [
        "apps", "packages", "platform", "servers", "services", "workers",
    ] {
        let base = root.join(scope);
        if !base.exists() {
            continue;
        }
        for manifest in collect_files_named(&base, "Cargo.toml") {
            let Some(manifest_dir) = manifest.parent() else {
                continue;
            };
            let source_path = normalize_slashes(manifest_dir.strip_prefix(&root)?);
            let family = source_path.split('/').next().unwrap_or_default();
            let manifest_toml: toml::Value = toml::from_str(&read(&manifest)?)?;
            let mut dependencies = Vec::new();
            for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
                let Some(table) = manifest_toml.get(section).and_then(toml::Value::as_table) else {
                    continue;
                };
                for (dependency_name, value) in table {
                    let Some(dep_table) = value.as_table() else {
                        continue;
                    };
                    if let Some(path) = dep_table.get("path").and_then(toml::Value::as_str) {
                        dependencies.push(normalize_slashes(
                            manifest_dir.join(path).strip_prefix(&root)?,
                        ));
                    } else if dep_table.get("workspace").and_then(toml::Value::as_bool)
                        == Some(true)
                    {
                        if let Some(path) = workspace_paths.get(dependency_name) {
                            dependencies.push(path.clone());
                        }
                    }
                }
            }
            dependencies.sort();
            dependencies.dedup();

            for rule in &rules {
                let Some(rule_map) = rule.as_mapping() else {
                    continue;
                };
                let from_patterns = yaml_patterns(rule_map.get(serde_yaml::Value::from("from")));
                if !from_patterns
                    .iter()
                    .any(|pattern| pattern_matches(&format!("{family}/**"), pattern))
                {
                    continue;
                }
                let disallow = yaml_patterns(rule_map.get(serde_yaml::Value::from("disallow")));
                let allow = yaml_patterns(rule_map.get(serde_yaml::Value::from("allow")));
                let except = yaml_patterns(rule_map.get(serde_yaml::Value::from("except")));
                let except_same_module = rule_map
                    .get(serde_yaml::Value::from("except_same_module"))
                    .and_then(serde_yaml::Value::as_bool)
                    .unwrap_or(false);
                let rule_name = rule_map
                    .get(serde_yaml::Value::from("name"))
                    .and_then(serde_yaml::Value::as_str)
                    .unwrap_or("unnamed");

                for dependency in &dependencies {
                    if except_same_module && same_module(&source_path, dependency) {
                        continue;
                    }
                    if except_path(&source_path, dependency, &except) {
                        continue;
                    }
                    if disallow
                        .iter()
                        .any(|pattern| pattern_matches(dependency, pattern))
                    {
                        issues.push((
                            mode.is_strict(),
                            source_path.clone(),
                            format!("depends on forbidden path {dependency} (rule: {rule_name})"),
                        ));
                        continue;
                    }
                    if !allow.is_empty() {
                        let same_family = dependency.starts_with(&format!("{family}/"));
                        let allowed = same_family
                            || allow
                                .iter()
                                .any(|pattern| pattern_matches(dependency, pattern));
                        if !allowed {
                            issues.push((mode.is_strict(), source_path.clone(), format!("depends on path outside allowlist: {dependency} (rule: {rule_name})")));
                        }
                    }
                }
            }
        }
    }

    let mut report = Report::new("validate-imports", mode);
    report.extend(issues.iter().map(|(error, scope, message)| {
        if *error {
            Issue::error(scope.clone(), message.clone())
        } else {
            Issue::warn(scope.clone(), message.clone())
        }
    }));
    report.print();
    if issues.is_empty() {
        println!("No import rule issues found");
        return Ok(());
    }
    report.exit_if_needed();
    Ok(())
}

pub(crate) fn validate_publish_intent(mode: Mode) -> Result<()> {
    let root = workspace_root()?;
    let metadata = cargo_metadata(&root)?;
    let workspace_members = metadata
        .get("workspace_members")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut package_by_id = BTreeMap::new();
    for package in packages {
        let Some(id) = package.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        package_by_id.insert(id.to_string(), package);
    }

    let mut issues = Vec::new();
    for member in workspace_members {
        let Some(id) = member.as_str() else {
            continue;
        };
        let Some(package) = package_by_id.get(id) else {
            continue;
        };
        let name = package
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unknown>");
        let manifest_path = package
            .get("manifest_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unknown>");
        let scope = if manifest_path == "<unknown>" {
            format!("{name}:<unknown>")
        } else {
            let path = std::path::Path::new(manifest_path);
            let relative = path.strip_prefix(&root).unwrap_or(path);
            format!("{}:{}", name, normalize_slashes(relative))
        };

        match package.get("publish") {
            Some(serde_json::Value::Array(values)) if values.is_empty() => {}
            Some(serde_json::Value::Array(values)) => {
                issues.push((
                    mode.is_strict(),
                    scope,
                    format!(
                        "workspace package is publishable to registry allowlist {:?}; Phase 0 policy requires publish=false",
                        values
                    ),
                ));
            }
            Some(serde_json::Value::Null) | None => {
                issues.push((
                    mode.is_strict(),
                    scope,
                    "missing explicit publish=false intent".to_string(),
                ));
            }
            Some(other) => {
                issues.push((
                    mode.is_strict(),
                    scope,
                    format!("unexpected publish metadata shape: {other}"),
                ));
            }
        }
    }

    let mut report = Report::new("validate-publish-intent", mode);
    report.extend(issues.iter().map(|(error, scope, message)| {
        if *error {
            Issue::error(scope.clone(), message.clone())
        } else {
            Issue::warn(scope.clone(), message.clone())
        }
    }));
    report.print();
    if issues.is_empty() {
        println!("All workspace packages explicitly declare publish=false");
        return Ok(());
    }
    report.exit_if_needed();
    Ok(())
}

fn yaml_patterns(value: Option<&serde_yaml::Value>) -> Vec<String> {
    match value {
        Some(serde_yaml::Value::Sequence(sequence)) => sequence
            .iter()
            .filter_map(serde_yaml::Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        Some(other) => other
            .as_str()
            .map(|value| vec![value.to_string()])
            .unwrap_or_default(),
        None => Vec::new(),
    }
}
