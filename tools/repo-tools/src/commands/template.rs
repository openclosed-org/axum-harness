use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::{
    AuditBackendCoreArgs, BackendCoreAuditMode, SemverCheckArgs, TemplateInitArgs,
    TemplateInitMode, TemplateProfile,
};
use crate::support::{normalize_slashes, read, run_capture, workspace_root, write};

struct TemplatePlan {
    keep: &'static [&'static str],
    review: &'static [&'static str],
    remove_candidates: &'static [&'static str],
}

pub(crate) fn template_init(args: TemplateInitArgs) -> Result<()> {
    let plan = template_plan(args.profile);

    println!("=== Template Init Plan ===");
    println!("profile: {}", args.profile.label());
    println!("mode: {}", args.mode.label());
    println!("scope: backend-core template cleanup");

    print_list("Keep", plan.keep);
    print_list("Review manually", plan.review);
    print_list("Removal candidates", plan.remove_candidates);

    if args.mode == TemplateInitMode::Apply {
        ensure_safe_to_apply()?;
        println!("\nApplying removal candidates:");
        let root = workspace_root()?;
        for pattern in plan.remove_candidates {
            remove_path_pattern(&root, pattern)?;
        }
        if args.profile == TemplateProfile::BackendCore {
            remove_repository_release_anchor(&root)?;
        }
        println!("\nApply complete. Run `just audit-backend-core` and `just verify` next.");
        return Ok(());
    }

    println!("\nDry-run only. No files were changed.");
    println!(
        "Run `just template-init {} apply` only after reviewing the removal candidates.",
        args.profile.label()
    );
    println!("See docs/template-users/template-init.md for the current design contract.");
    Ok(())
}

pub(crate) fn audit_backend_core(args: AuditBackendCoreArgs) -> Result<()> {
    let root = workspace_root()?;
    let findings = backend_entry_files()
        .iter()
        .flat_map(|file| scan_backend_entry_file(&root, file).unwrap_or_default())
        .collect::<Vec<_>>();

    println!("=== Backend Core Audit ===");
    println!("mode: {}", args.mode.label());
    println!("scope: root backend-core contract");
    println!("scope: backend-only root command contract");

    if findings.is_empty() {
        println!("\nPASS: root backend-core contract is free of frontend shell references.");
        println!("Next proof command: just verify");
        return Ok(());
    }

    println!("\nFindings:");
    for finding in &findings {
        println!(
            "  - {}: {} :: {}",
            finding.file, finding.needle, finding.context
        );
    }

    if args.mode == BackendCoreAuditMode::Strict {
        bail!("backend-core audit found {} finding(s)", findings.len());
    }

    println!("\nDry-run reported findings only. Use strict mode to fail on findings.");
    Ok(())
}

pub(crate) fn semver_check(args: SemverCheckArgs) -> Result<()> {
    let root = workspace_root()?;
    let baseline = if args.baseline.is_empty() {
        latest_tag(&root, &args.release_tag_glob)?
    } else {
        Some(args.baseline)
    };

    let Some(baseline) = baseline else {
        println!(
            "No repository semver tag found; skipping SemVer baseline check until the first official template release exists."
        );
        return Ok(());
    };

    let tag_exists = run_capture(
        "git",
        &[
            "rev-parse",
            "-q",
            "--verify",
            &format!("refs/tags/{baseline}"),
        ],
        Some(&root),
    )?;
    if !tag_exists.success {
        bail!("configured SemVer baseline tag does not exist: {baseline}");
    }

    println!("Using SemVer baseline: {baseline}");
    println!("Using release type: {}", args.release_type);
    println!("Using release tag glob: {}", args.release_tag_glob);

    let metadata = crate::support::cargo_metadata(&root)?;
    let packages = metadata["packages"]
        .as_array()
        .context("cargo metadata missing packages array")?;
    for package_name in [
        "contracts_api",
        "contracts_events",
        "contracts_errors",
        "contracts_auth",
    ] {
        let manifest_path = packages
            .iter()
            .find(|package| package["name"].as_str() == Some(package_name))
            .and_then(|package| package["manifest_path"].as_str());
        let Some(manifest_path) = manifest_path else {
            println!("Skipping {package_name}: package not found in current cargo metadata");
            continue;
        };
        let manifest_rel_path = normalize_slashes(Path::new(manifest_path).strip_prefix(&root)?);
        let exists_at_baseline = run_capture(
            "git",
            &["cat-file", "-e", &format!("{baseline}:{manifest_rel_path}")],
            Some(&root),
        )?;
        if !exists_at_baseline.success {
            println!("Skipping {package_name}: package did not exist at baseline {baseline}");
            continue;
        }
        let status = crate::support::run_inherit(
            "cargo",
            &[
                "semver-checks",
                "--baseline-rev",
                &baseline,
                "--release-type",
                &args.release_type,
                "--package",
                package_name,
            ],
            Some(&root),
        )?;
        if status != 0 {
            bail!("cargo semver-checks failed for {package_name}");
        }
    }
    Ok(())
}

fn latest_tag(root: &Path, glob: &str) -> Result<Option<String>> {
    let tags = run_capture("git", &["tag", "-l", glob], Some(root))?;
    if !tags.success {
        bail!("failed to list git tags: {}", tags.error);
    }
    let mut tag_list = tags
        .output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    tag_list.sort_by(|left, right| compare_version_tags(left, right));
    Ok(tag_list.pop())
}

fn compare_version_tags(left: &str, right: &str) -> std::cmp::Ordering {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    left_parts.cmp(&right_parts).then_with(|| left.cmp(right))
}

fn version_parts(tag: &str) -> Vec<u64> {
    tag.trim_start_matches('v')
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn template_plan(profile: TemplateProfile) -> TemplatePlan {
    match profile {
        TemplateProfile::BackendCore => TemplatePlan {
            keep: &[
                "README.md",
                "LICENSE",
                "AGENTS.md",
                "agent/**",
                "docs/operations/**",
                "docs/contracts/README.md",
                "services/counter-service/**",
                "servers/bff/web-bff/**",
                "workers/outbox-relay/**",
                "workers/projector/**",
                "packages/contracts/**",
                "packages/kernel/**",
                "packages/platform/**",
                "packages/messaging/**",
                "packages/data/**",
                "packages/data-traits/**",
                "packages/data-adapters/turso/**",
                "packages/observability/**",
                "infra/**",
                "Justfile",
                "moon.yml",
                "justfiles/core/**",
                "justfiles/quality/**",
                "justfiles/domains/**",
                "justfiles/ops/**",
                "justfiles/agent/**",
                "tools/repo-tools/**",
            ],
            review: &[
                "CONTRIBUTING.md",
                "CODE_OF_CONDUCT.md",
                "docs/template-users/**",
                ".github/workflows/**",
                "verification/**",
                "services/tenant-service/**",
            ],
            remove_candidates: &[
                "verification/e2e/**",
                "docs/governance/**",
                "docs/archive/**",
                "release-plz.toml",
                "release-plz.template.toml",
                ".github/workflows/release-plz.yml",
                "tools/repo-release/**",
                ".github/ISSUE_TEMPLATE/**",
                ".github/pull_request_template.md",
            ],
        },
        TemplateProfile::FullResearch => TemplatePlan {
            keep: &["entire repository"],
            review: &[],
            remove_candidates: &[],
        },
    }
}

fn print_list(title: &str, values: &[&str]) {
    println!("\n{title}");
    if values.is_empty() {
        println!("  (none)");
        return;
    }
    for value in values {
        println!("  - {value}");
    }
}

fn ensure_safe_to_apply() -> Result<()> {
    if std::env::var("TEMPLATE_INIT_ALLOW_DIRTY").as_deref() == Ok("1") {
        println!("\nDirty worktree check bypassed by TEMPLATE_INIT_ALLOW_DIRTY=1.");
        return Ok(());
    }

    let root = workspace_root()?;
    let git_probe = run_capture("git", &["rev-parse", "--is-inside-work-tree"], Some(&root))?;
    if !git_probe.success || git_probe.output.trim() != "true" {
        println!("\nNo git worktree detected; skipping dirty worktree check.");
        return Ok(());
    }

    let status = run_capture("git", &["status", "--porcelain"], Some(&root))?;
    if !status.success {
        bail!("unable to inspect git worktree status; refusing to apply cleanup");
    }
    if !status.output.trim().is_empty() {
        bail!(
            "refusing to apply template cleanup with a dirty worktree; commit or stash local changes first, or set TEMPLATE_INIT_ALLOW_DIRTY=1 after reviewing the risk"
        );
    }
    Ok(())
}

fn remove_path_pattern(root: &std::path::Path, pattern: &str) -> Result<()> {
    let path = pattern.strip_suffix("/**").unwrap_or(pattern);
    let target = root.join(path);
    if target.is_dir() {
        fs::remove_dir_all(&target).with_context(|| format!("failed to remove {path}"))?;
    } else if target.exists() {
        fs::remove_file(&target).with_context(|| format!("failed to remove {path}"))?;
    }
    println!("  removed {path}");
    Ok(())
}

fn remove_repository_release_anchor(root: &std::path::Path) -> Result<()> {
    let cargo_toml_path = root.join("Cargo.toml");
    if !cargo_toml_path.exists() {
        return Ok(());
    }
    let original = read(&cargo_toml_path)?;
    let package_anchor = "[package]\nname = \"axum-harness\"\n";
    let workspace_anchor = "\n[workspace]\n";
    if !original.starts_with(package_anchor) {
        return Ok(());
    }
    let lib_re = regex::Regex::new(
        r#"\n\[lib\]\npath = "tools/repo-release/src/lib\.rs"\n\n\[lints\]\nworkspace = true\n"#,
    )?;
    let Some(workspace_start) = original.find(workspace_anchor) else {
        return Ok(());
    };
    let next = format!("{}{}", &original[..0], &original[workspace_start + 1..]);
    let next = lib_re.replace(&next, "\n");
    if next != original {
        write(&cargo_toml_path, next.as_ref())?;
        println!("  removed repository release anchor from Cargo.toml");
    }
    Ok(())
}

struct Finding {
    file: String,
    needle: &'static str,
    context: String,
}

fn backend_entry_files() -> &'static [&'static str] {
    &[
        ".moon/workspace.yml",
        "moon.yml",
        "Justfile",
        "justfiles/core/setup.just",
        "justfiles/core/build.just",
        "justfiles/core/dev.just",
        "justfiles/core/clean.just",
        "justfiles/quality/verify.just",
        "justfiles/quality/gates.just",
        "justfiles/domains/backend.just",
        "justfiles/domains/contracts.just",
        "justfiles/domains/platform.just",
        "justfiles/ops/deploy.just",
        "justfiles/ops/sops.just",
        "justfiles/agent/template.just",
    ]
}

fn forbidden_needles() -> &'static [&'static str] {
    &[
        "apps/web",
        "apps/desktop",
        "apps/mobile",
        "apps/browser-extension",
        "packages/ui",
        "web:",
        "apps/client",
    ]
}

fn scan_backend_entry_file(root: &std::path::Path, file: &str) -> Result<Vec<Finding>> {
    let path = root.join(file);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut findings = Vec::new();
    for (index, line) in read(&path)?.lines().enumerate() {
        for needle in forbidden_needles() {
            if line.contains(needle) {
                findings.push(Finding {
                    file: format!(
                        "{}:{}",
                        normalize_slashes(std::path::Path::new(file)),
                        index + 1
                    ),
                    needle,
                    context: line.trim().to_string(),
                });
            }
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_entry_files_include_root_justfile_with_correct_case() {
        let files = backend_entry_files();

        assert!(files.contains(&"Justfile"));
        assert!(!files.contains(&"justfile"));
    }

    #[test]
    fn backend_core_template_plan_uses_current_justfile_modules() {
        let plan = template_plan(TemplateProfile::BackendCore);

        assert!(plan.keep.contains(&"justfiles/core/**"));
        assert!(plan.keep.contains(&"justfiles/quality/**"));
        assert!(plan.keep.contains(&"justfiles/domains/**"));
        assert!(plan.keep.contains(&"justfiles/ops/**"));
        assert!(plan.keep.contains(&"justfiles/agent/**"));
        assert!(plan.keep.contains(&"tools/repo-tools/**"));
        assert!(!plan.keep.contains(&"justfiles/test.just"));
        assert!(!plan.keep.contains(&"justfiles/quality.just"));
    }
}
