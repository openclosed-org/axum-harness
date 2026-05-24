use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};

use crate::cli::{GateGuidanceArgs, RouteTaskArgs};
use crate::core::manifest::{load_gate_matrix, load_routing_rules};
use crate::support::{git_changed_paths, pattern_matches, run_capture, workspace_root};

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
    let mut affected_domains = affected.to_vec();
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
