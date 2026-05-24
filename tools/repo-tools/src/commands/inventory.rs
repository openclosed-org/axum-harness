use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::support::{normalize_slashes, run_capture, workspace_root, write};

pub(crate) fn audit_inventory() -> Result<()> {
    let root = workspace_root()?;
    let tracked_output = run_capture("git", &["ls-files"], Some(&root))?;
    let tracked_files = if tracked_output.success {
        tracked_output
            .output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };

    let ignored_output = run_capture(
        "git",
        &["ls-files", "--others", "--ignored", "--exclude-standard"],
        Some(&root),
    )?;
    let ignored_files = if ignored_output.success {
        ignored_output
            .output
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>()
    } else {
        BTreeSet::new()
    };

    let mut entries = Vec::new();
    let mut total_files = 0usize;
    let mut total_dirs = 0usize;
    for entry in walkdir::WalkDir::new(&root)
        .into_iter()
        .filter_entry(|entry| should_descend(entry.path(), &root))
    {
        let entry = entry?;
        if entry.path() == root {
            continue;
        }
        let relative = normalize_slashes(entry.path().strip_prefix(&root)?);
        if entry.file_type().is_file() {
            total_files += 1;
        } else if entry.file_type().is_dir() {
            total_dirs += 1;
        } else {
            continue;
        }
        let status = classify_inventory_status(&relative, &tracked_files, &ignored_files);
        entries.push(InventoryEntry {
            path: relative.clone(),
            kind: if entry.file_type().is_dir() {
                "dir"
            } else {
                "file"
            }
            .to_string(),
            top_level: relative.split('/').next().unwrap_or_default().to_string(),
            status,
        });
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let summary = InventorySummary {
        total_files,
        total_dirs,
        tracked_files: tracked_files.len(),
        ignored_files: ignored_files.len(),
    };
    let report = InventoryReport {
        version: 1,
        generated_by: "repo-tools audit inventory".to_string(),
        summary,
        entries,
    };
    let output_path = root.join("target/audit/repo-inventory.json");
    write(&output_path, &serde_json::to_string_pretty(&report)?)?;
    println!("Wrote {}", output_path.display());
    println!(
        "Inventory: {} file(s), {} dir(s), {} tracked file(s), {} ignored file(s)",
        report.summary.total_files,
        report.summary.total_dirs,
        report.summary.tracked_files,
        report.summary.ignored_files
    );
    Ok(())
}

fn should_descend(path: &Path, root: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let first = relative
        .components()
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .unwrap_or_default();
    !matches!(first, ".git" | "target" | "node_modules")
}

fn classify_inventory_status(
    relative: &str,
    tracked_files: &BTreeSet<String>,
    ignored_files: &BTreeSet<String>,
) -> String {
    if tracked_files.contains(relative) {
        return "tracked".to_string();
    }
    if ignored_files.contains(relative)
        || ignored_files
            .iter()
            .any(|path| path.starts_with(&format!("{relative}/")))
    {
        return "ignored_or_contains_ignored".to_string();
    }
    "untracked_or_directory".to_string()
}

#[derive(Serialize)]
struct InventoryReport {
    version: u8,
    generated_by: String,
    summary: InventorySummary,
    entries: Vec<InventoryEntry>,
}

#[derive(Serialize)]
struct InventorySummary {
    total_files: usize,
    total_dirs: usize,
    tracked_files: usize,
    ignored_files: usize,
}

#[derive(Serialize)]
struct InventoryEntry {
    path: String,
    kind: String,
    top_level: String,
    status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_inventory_status_uses_tracked_before_ignored() {
        let tracked = BTreeSet::from(["agent/codemap.yml".to_string()]);
        let ignored = BTreeSet::from(["target/audit/repo-inventory.json".to_string()]);

        assert_eq!(
            classify_inventory_status("agent/codemap.yml", &tracked, &ignored),
            "tracked"
        );
        assert_eq!(
            classify_inventory_status("target/audit", &tracked, &ignored),
            "ignored_or_contains_ignored"
        );
        assert_eq!(
            classify_inventory_status("docs/_local/note.md", &tracked, &ignored),
            "untracked_or_directory"
        );
    }
}
