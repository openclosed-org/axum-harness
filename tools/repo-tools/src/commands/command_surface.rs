use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Result, bail};
use serde::Serialize;

use crate::core::{action, gate_catalog};
use crate::support::{Issue, Mode, Report, read, run_capture, workspace_root, write};

pub(crate) fn validate_justfiles_taxonomy(mode: Mode) -> Result<()> {
    let root = workspace_root()?;
    let mut issues = registry_issues();

    for path in [
        "Justfile",
        "justfiles/README.md",
        "agent/architecture/gate-taxonomy.yml",
    ] {
        if !root.join(path).exists() {
            issues.push(Issue::error(
                path,
                "required command-surface control file is missing",
            ));
        }
    }

    let expected_dirs = expected_justfiles_dirs(&root);
    for allowed in expected_dirs {
        if !root.join(&allowed).exists() {
            issues.push(Issue::warn(
                allowed,
                "expected command-surface layout directory is missing",
            ));
        }
    }

    let recipes = list_just_recipes(&root)?;
    write_justfiles_taxonomy_report(&root, &recipes)?;
    let forbidden = [
        "run-all",
        "do-check",
        "my-test",
        "test2",
        "full",
        "misc",
        "temp",
        "verify-stuff",
    ];
    let allowed_prefixes = [
        "fmt",
        "lint",
        "check",
        "validate",
        "test",
        "coverage",
        "mutants",
        "smoke",
        "audit",
        "render",
        "deploy",
        "gate",
        "agent",
        "verify",
        "typecheck",
        "boundary-check",
        "sops",
        "podman",
        "k3d",
        "multipass",
        "platform",
        "release",
        "clean",
        "setup",
        "doctor",
        "dev",
        "help",
        "logs",
        "status",
        "health",
        "ps",
        "migrate",
        "generate",
        "storage",
        "semver",
        "typegen",
        "drift",
        "sdk",
        "gen",
        "k6",
        "default",
    ];

    for recipe in &recipes {
        if let Some(action) = action::find_by_just_recipe(recipe) {
            let mut issue = Issue::info(
                recipe.clone(),
                format!(
                    "registered action {}; target {}",
                    action.id,
                    action::suggested_justfile_path(action)
                ),
            );
            issue.evidence = Some(format!("repo-tools {}", action.canonical_cli.join(" ")));
            issues.push(issue);
        }

        if forbidden.contains(&recipe.as_str()) {
            issues.push(Issue::error(
                recipe.clone(),
                "recipe name is forbidden by gate taxonomy",
            ));
        }
        if !allowed_prefixes.iter().any(|prefix| {
            recipe == prefix
                || recipe.starts_with(&format!("{prefix}-"))
                || recipe.starts_with(&format!("_{prefix}"))
        }) {
            issues.push(Issue::warn(
                recipe.clone(),
                "recipe does not match a known taxonomy prefix; classify before using as gate proof",
            ));
        }
    }

    for recipe in recipes.iter().filter(|recipe| recipe.starts_with("deploy")) {
        let has_match = recipes.iter().any(|candidate| {
            candidate.contains("dry-run")
                || candidate.starts_with("smoke")
                || candidate.starts_with("validate")
                || candidate.contains(recipe.trim_start_matches("deploy"))
        });
        if !has_match {
            issues.push(Issue::warn(
                recipe.clone(),
                "deploy recipe should have a matching validate, dry-run, or smoke recipe",
            ));
        }
    }

    let mut report = Report::new("validate-justfiles-taxonomy", mode);
    report.extend(issues);
    report.print();
    if report.is_empty() {
        println!("Justfiles command surface matches the initial taxonomy guardrails");
        return Ok(());
    }
    report.exit_if_needed();
    Ok(())
}

pub(crate) fn validate_new_command_taxonomy(mode: Mode) -> Result<()> {
    let root = workspace_root()?;
    let recipes = list_just_recipes(&root)?;
    let mut issues = registry_issues();

    for action in action::registry() {
        validate_registered_action_facade(action, &recipes, &mut issues);
    }

    if root
        .join("tools/repo-tools/src/commands/harness.rs")
        .exists()
    {
        issues.push(Issue::error(
            "tools/repo-tools/src/commands/harness.rs",
            "generic harness command bucket is forbidden; add commands to a named responsibility module",
        ));
    }
    validate_gate_command_shape(&root, &mut issues)?;

    if action::find_by_just_recipe("audit-command-surface").is_none() {
        issues.push(Issue::error(
            "audit-command-surface",
            "new command must be registered before strict admission",
        ));
    }

    let mut report = Report::new("validate-new-command-taxonomy", mode);
    report.extend(issues);
    report.print();
    if report.is_empty() {
        println!("New command taxonomy is admissible");
        return Ok(());
    }
    report.exit_if_needed();
    Ok(())
}

fn validate_gate_command_shape(root: &Path, issues: &mut Vec<Issue>) -> Result<()> {
    let gate_command_path = root.join("tools/repo-tools/src/commands/gate.rs");
    let gate_catalog_path = root.join("tools/repo-tools/src/core/gate_catalog.rs");

    if !gate_catalog_path.exists() {
        issues.push(Issue::error(
            "tools/repo-tools/src/core/gate_catalog.rs",
            "gate semantics must live in the typed gate catalog",
        ));
        return Ok(());
    }
    if !gate_command_path.exists() {
        issues.push(Issue::error(
            "tools/repo-tools/src/commands/gate.rs",
            "gate command interpreter is missing",
        ));
        return Ok(());
    }

    let gate_command = read(&gate_command_path)?;
    if gate_command.contains("match args.gate") || gate_command.contains("GateName::") {
        issues.push(Issue::error(
            "tools/repo-tools/src/commands/gate.rs",
            "gate command must interpret core/gate_catalog.rs instead of branching on gate names",
        ));
    }

    let gate_catalog = read(&gate_catalog_path)?;
    for legacy_cli in [
        "CommandSpec::RepoTools(&[\"boundary-check\"",
        "CommandSpec::RepoTools(&[\"audit-inventory\"",
        "CommandSpec::RepoTools(&[\"validate-agent-architecture\"",
        "CommandSpec::RepoTools(&[\"validate-justfiles-taxonomy\"",
        "CommandSpec::RepoTools(&[\"validate-new-command-taxonomy\"",
    ] {
        if gate_catalog.contains(legacy_cli) {
            issues.push(Issue::error(
                "tools/repo-tools/src/core/gate_catalog.rs",
                "gate catalog must use canonical repo-tools CLI paths, not flat legacy aliases",
            ));
        }
    }

    Ok(())
}

pub(crate) fn audit_command_surface() -> Result<()> {
    let root = workspace_root()?;
    let recipes = list_just_recipes(&root)?;
    let repo_tools_commands = list_repo_tools_commands(&root)?;
    let mut just_recipes = Vec::new();
    let mut facade_mappings = Vec::new();
    let mut just_without_repo_tools_backing = Vec::new();
    let mut shell_heavy_recipes = Vec::new();
    let mut high_risk_operations = Vec::new();

    for recipe in &recipes {
        let action = action::find_by_just_recipe(recipe);
        let migration_class = classify_recipe_migration(recipe, action);
        let suggested_justfile = action.map(action::suggested_justfile_path);
        let entry = JustRecipeSurfaceEntry {
            recipe: recipe.clone(),
            registered_action: action.map(|action| action.id.to_string()),
            suggested_justfile: suggested_justfile.map(ToOwned::to_owned),
            migration_class: migration_class.to_string(),
        };

        if let Some(action) = action {
            facade_mappings.push(FacadeMapping {
                just_recipe: recipe.clone(),
                action_id: action.id.to_string(),
                repo_tools_cli: action.canonical_cli.join(" "),
                suggested_justfile: action::suggested_justfile_path(action).to_string(),
            });
            if action.side_effect.is_high_risk() {
                high_risk_operations.push(HighRiskOperation {
                    id: action.id.to_string(),
                    side_effect: action.side_effect.label().to_string(),
                    agent_auto_run: action.agent_auto_run.label().to_string(),
                });
            }
        } else {
            just_without_repo_tools_backing.push(recipe.clone());
        }

        if migration_class == "shell_logic_to_repo_tools" {
            shell_heavy_recipes.push(recipe.clone());
        }
        just_recipes.push(entry);
    }

    let registered_cli = action::registry()
        .iter()
        .map(|action| action.canonical_cli.join(" "))
        .collect::<BTreeSet<_>>();
    let repo_tools_without_just_facade = action::registry()
        .iter()
        .filter(|action| action.just_recipe.is_none())
        .map(|action| action.canonical_cli.join(" "))
        .collect::<Vec<_>>();
    let repo_tools_commands = repo_tools_commands
        .into_iter()
        .map(|command| RepoToolsCommandEntry {
            command: command.clone(),
            registered_action: registered_cli.contains(&command),
        })
        .collect::<Vec<_>>();

    let report = CommandSurfaceReport {
        version: 1,
        generated_by: "repo-tools audit-command-surface".to_string(),
        summary: CommandSurfaceSummary {
            just_recipes: just_recipes.len(),
            repo_tools_commands: repo_tools_commands.len(),
            registered_actions: action::registry().len(),
            facade_mappings: facade_mappings.len(),
            just_without_repo_tools_backing: just_without_repo_tools_backing.len(),
            shell_heavy_recipes: shell_heavy_recipes.len(),
            high_risk_operations: high_risk_operations.len(),
        },
        just_recipes,
        repo_tools_commands,
        facade_mappings,
        repo_tools_without_just_facade,
        just_without_repo_tools_backing,
        shell_heavy_recipes,
        high_risk_operations,
        registered_actions: registered_action_entries(),
    };

    let output_path = root.join("target/audit/command-surface.json");
    write(&output_path, &serde_json::to_string_pretty(&report)?)?;
    println!("Wrote {}", output_path.display());
    println!(
        "Command surface: {} just recipe(s), {} repo-tools command(s), {} registered action(s), {} facade mapping(s)",
        report.summary.just_recipes,
        report.summary.repo_tools_commands,
        report.summary.registered_actions,
        report.summary.facade_mappings,
    );
    Ok(())
}

fn registry_issues() -> Vec<Issue> {
    let mut issues = action::registry_invariant_issues()
        .into_iter()
        .map(|issue| Issue::error("action-registry", issue))
        .collect::<Vec<_>>();

    issues.extend(
        gate_catalog::gate_catalog_invariant_issues()
            .into_iter()
            .map(|issue| Issue::error("gate-catalog", issue)),
    );

    for action_id in REQUIRED_ACTION_IDS {
        if action::find_by_id(action_id).is_none() {
            issues.push(Issue::error(
                "action-registry",
                format!("required seed action {action_id:?} is missing"),
            ));
        }
    }
    issues
}

fn validate_registered_action_facade(
    action: &action::ActionSpec,
    recipes: &BTreeSet<String>,
    issues: &mut Vec<Issue>,
) {
    if let Some(recipe) = action.just_recipe {
        if !recipes.contains(recipe) {
            issues.push(Issue::error(
                recipe,
                format!("registered action {} has no just facade", action.id),
            ));
        }
    }

    if !action.legacy_cli_aliases.is_empty() {
        issues.push(Issue::error(
            action.id,
            "legacy repo-tools CLI aliases are not allowed; keep compatibility at just facade level",
        ));
    }
}

fn expected_justfiles_dirs(root: &Path) -> Vec<String> {
    let target_dirs = ["check", "maintain"];
    if target_dirs
        .iter()
        .any(|directory| root.join("justfiles").join(directory).exists())
    {
        return target_dirs
            .into_iter()
            .map(|directory| format!("justfiles/{directory}"))
            .collect();
    }
    ["core", "quality", "domains", "ops", "agent"]
        .into_iter()
        .map(|directory| format!("justfiles/{directory}"))
        .collect()
}

fn write_justfiles_taxonomy_report(root: &Path, recipes: &BTreeSet<String>) -> Result<()> {
    let entries = recipes
        .iter()
        .map(|recipe| {
            let action = action::find_by_just_recipe(recipe);
            JustfilesTaxonomyEntry {
                recipe: recipe.clone(),
                registered_action: action.map(|action| action.id.to_string()),
                action_class: action.map(|action| action.class.label().to_string()),
                domain: action.map(|action| action.domain.label().to_string()),
                side_effect: action.map(|action| action.side_effect.label().to_string()),
                evidence_role: action.map(|action| action.evidence_role.label().to_string()),
                suggested_justfile: action
                    .map(action::suggested_justfile_path)
                    .map(ToOwned::to_owned),
                migration_class: classify_recipe_migration(recipe, action).to_string(),
            }
        })
        .collect::<Vec<_>>();
    let report = JustfilesTaxonomyReport {
        version: 1,
        generated_by: "repo-tools validate justfiles-taxonomy".to_string(),
        summary: JustfilesTaxonomySummary {
            recipes: entries.len(),
            registered_actions: entries
                .iter()
                .filter(|entry| entry.registered_action.is_some())
                .count(),
        },
        entries,
    };
    let output_path = root.join("target/audit/justfiles-taxonomy.json");
    write(&output_path, &serde_json::to_string_pretty(&report)?)?;
    Ok(())
}

fn list_just_recipes(root: &Path) -> Result<BTreeSet<String>> {
    let list = run_capture("just", &["--list"], Some(root))?;
    if !list.success {
        bail!("failed to list just recipes for command-surface audit");
    }
    Ok(parse_just_list(&list.output))
}

fn list_repo_tools_commands(root: &Path) -> Result<Vec<String>> {
    let output = run_capture(
        "cargo",
        &["run", "-p", "repo-tools", "--", "--help"],
        Some(root),
    )?;
    if !output.success {
        bail!("failed to list repo-tools commands for command-surface audit");
    }
    let mut commands = parse_clap_commands(&output.output)
        .into_iter()
        .chain(
            action::registry()
                .iter()
                .map(|action| action.canonical_cli.join(" ")),
        )
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    commands.sort();
    Ok(commands)
}

fn parse_clap_commands(output: &str) -> BTreeSet<String> {
    let mut in_commands = false;
    let mut commands = BTreeSet::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed == "Commands:" {
            in_commands = true;
            continue;
        }
        if in_commands && trimmed.is_empty() {
            break;
        }
        if !in_commands {
            continue;
        }
        if let Some(command) = trimmed.split_whitespace().next() {
            if command
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
            {
                commands.insert(command.to_string());
            }
        }
    }
    commands
}

fn parse_just_list(output: &str) -> BTreeSet<String> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let starts_like_recipe = trimmed
                .chars()
                .next()
                .map(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                .unwrap_or(false);
            if trimmed.is_empty() || trimmed.ends_with(':') || !starts_like_recipe {
                return None;
            }
            trimmed
                .split_whitespace()
                .next()
                .map(|recipe| recipe.trim_end_matches(':').to_string())
        })
        .collect()
}

fn classify_recipe_migration(recipe: &str, action: Option<&action::ActionSpec>) -> &'static str {
    if let Some(action) = action {
        if action.side_effect.is_high_risk() {
            return "high_risk_no_move";
        }
        return "obvious_move";
    }
    if is_high_risk_recipe_name(recipe) {
        return "high_risk_no_move";
    }
    if is_shell_heavy_recipe_name(recipe) {
        return "shell_logic_to_repo_tools";
    }
    if is_legacy_alias_recipe_name(recipe) {
        return "keep_legacy_alias";
    }
    "needs_decision"
}

fn is_high_risk_recipe_name(recipe: &str) -> bool {
    recipe.contains("delete")
        || recipe.contains("deletes-state")
        || recipe.contains("reset")
        || recipe.contains("prune-volumes")
        || recipe.starts_with("deploy")
        || recipe.starts_with("sops-edit")
        || recipe.starts_with("sops-export")
        || recipe.starts_with("sops-reconcile")
}

fn is_shell_heavy_recipe_name(recipe: &str) -> bool {
    recipe.starts_with("k3d-")
        || recipe.starts_with("podman-")
        || recipe.starts_with("auth-")
        || recipe.starts_with("migrate-")
        || recipe.starts_with("stop")
        || recipe.starts_with("logs-")
        || recipe.starts_with("status-")
        || recipe.starts_with("smoke-")
}

fn is_legacy_alias_recipe_name(recipe: &str) -> bool {
    matches!(
        recipe,
        "verify" | "typecheck" | "boundary-check" | "default"
    ) || recipe.starts_with("verify-")
}

fn registered_action_entries() -> Vec<RegisteredActionEntry> {
    action::registry()
        .iter()
        .map(|action| RegisteredActionEntry {
            id: action.id.to_string(),
            canonical_cli: action.canonical_cli.join(" "),
            legacy_cli_aliases: action
                .legacy_cli_aliases
                .iter()
                .map(|alias| alias.join(" "))
                .collect(),
            just_recipe: action.just_recipe.map(ToOwned::to_owned),
            action_class: action.class.label().to_string(),
            domain: action.domain.label().to_string(),
            side_effect: action.side_effect.label().to_string(),
            evidence_role: action.evidence_role.label().to_string(),
            cost_level: action.cost_level.label().to_string(),
            agent_auto_run: action.agent_auto_run.label().to_string(),
            ci_allowed: action.ci_allowed,
            release_blocking: action.release_blocking,
            output_contract: action.output_contract.label().to_string(),
            output_path: action.output_contract.path().map(ToOwned::to_owned),
            suggested_justfile: action::suggested_justfile_path(action).to_string(),
        })
        .collect()
}

const REQUIRED_ACTION_IDS: &[&str] = &[
    "validate-agent-architecture",
    "validate-justfiles-taxonomy",
    "audit-inventory",
    "audit-command-surface",
    "boundary-check",
    "sops-validate",
    "gate-local",
    "gate-ci",
];

#[derive(Serialize)]
struct CommandSurfaceReport {
    version: u8,
    generated_by: String,
    summary: CommandSurfaceSummary,
    just_recipes: Vec<JustRecipeSurfaceEntry>,
    repo_tools_commands: Vec<RepoToolsCommandEntry>,
    facade_mappings: Vec<FacadeMapping>,
    repo_tools_without_just_facade: Vec<String>,
    just_without_repo_tools_backing: Vec<String>,
    shell_heavy_recipes: Vec<String>,
    high_risk_operations: Vec<HighRiskOperation>,
    registered_actions: Vec<RegisteredActionEntry>,
}

#[derive(Serialize)]
struct CommandSurfaceSummary {
    just_recipes: usize,
    repo_tools_commands: usize,
    registered_actions: usize,
    facade_mappings: usize,
    just_without_repo_tools_backing: usize,
    shell_heavy_recipes: usize,
    high_risk_operations: usize,
}

#[derive(Serialize)]
struct JustRecipeSurfaceEntry {
    recipe: String,
    registered_action: Option<String>,
    suggested_justfile: Option<String>,
    migration_class: String,
}

#[derive(Serialize)]
struct RepoToolsCommandEntry {
    command: String,
    registered_action: bool,
}

#[derive(Serialize)]
struct FacadeMapping {
    just_recipe: String,
    action_id: String,
    repo_tools_cli: String,
    suggested_justfile: String,
}

#[derive(Serialize)]
struct HighRiskOperation {
    id: String,
    side_effect: String,
    agent_auto_run: String,
}

#[derive(Serialize)]
struct RegisteredActionEntry {
    id: String,
    canonical_cli: String,
    legacy_cli_aliases: Vec<String>,
    just_recipe: Option<String>,
    action_class: String,
    domain: String,
    side_effect: String,
    evidence_role: String,
    cost_level: String,
    agent_auto_run: String,
    ci_allowed: bool,
    release_blocking: bool,
    output_contract: String,
    output_path: Option<String>,
    suggested_justfile: String,
}

#[derive(Serialize)]
struct JustfilesTaxonomyReport {
    version: u8,
    generated_by: String,
    summary: JustfilesTaxonomySummary,
    entries: Vec<JustfilesTaxonomyEntry>,
}

#[derive(Serialize)]
struct JustfilesTaxonomySummary {
    recipes: usize,
    registered_actions: usize,
}

#[derive(Serialize)]
struct JustfilesTaxonomyEntry {
    recipe: String,
    registered_action: Option<String>,
    action_class: Option<String>,
    domain: Option<String>,
    side_effect: Option<String>,
    evidence_role: Option<String>,
    suggested_justfile: Option<String>,
    migration_class: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_just_list_extracts_recipe_names() {
        let output = r#"
Available recipes:
    check-backend-primary
    validate-agent-architecture MODE='warn'
    _require TOOL HINT
    smoke-local-k3d CLUSTER='axh-local'
"#;

        let recipes = parse_just_list(output);

        assert!(recipes.contains("check-backend-primary"));
        assert!(recipes.contains("validate-agent-architecture"));
        assert!(recipes.contains("_require"));
        assert!(recipes.contains("smoke-local-k3d"));
        assert!(!recipes.contains("Available"));
    }
}
