use anyhow::{Result, bail};

use crate::cli::GateArgs;
use crate::core::gate_catalog::{self, CommandSpec};
use crate::support::run_capture;

pub(crate) fn gate(args: GateArgs) -> Result<()> {
    let spec = gate_catalog::find_gate(args.gate);
    let mode = args.mode().unwrap_or(spec.default_mode);

    println!(
        "=== gate-{} ({}, cost: {}) ===",
        spec.id,
        if mode.is_strict() { "strict" } else { "warn" },
        spec.cost.label()
    );
    println!("Purpose: {}", spec.purpose);
    let mut failures = Vec::new();
    for step in spec.steps {
        let program = step.command.program();
        let args = command_args(step.command);
        println!("\n-> {}: {} {}", step.label, program, args.join(" "));
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let outcome = run_capture(program, &refs, None)?;
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
            bail!("gate-{} blocked by {}", spec.id, step.label);
        }
    }

    if failures.is_empty() {
        println!("\n✓ gate-{} passed", spec.id);
        return Ok(());
    }

    println!(
        "\n! gate-{} completed with {} warning(s)",
        spec.id,
        failures.len()
    );
    for (label, exit_code) in failures {
        println!("  - {label}: exit {exit_code}");
    }
    Ok(())
}

fn command_args(command: CommandSpec) -> Vec<String> {
    let release_type = std::env::var("RELEASE_TYPE").unwrap_or_else(|_| "minor".to_string());
    command
        .args()
        .into_iter()
        .map(|arg| {
            if arg == "$RELEASE_TYPE" {
                release_type.clone()
            } else {
                arg
            }
        })
        .collect()
}
