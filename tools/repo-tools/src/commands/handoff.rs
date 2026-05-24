use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::cli::{GateGuidanceArgs, VerifyHandoffArgs};
use crate::commands::routing;
use crate::core::manifest::load_codemap;
use crate::support::{run_capture, workspace_root};

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

    println!("\n--- Gate Selection Guidance ---");
    routing::gate_guidance(GateGuidanceArgs {
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
