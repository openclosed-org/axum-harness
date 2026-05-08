use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Result;
use regex::Regex;
use serde::Deserialize;

use crate::support::{Issue, Mode, Report, collect_files_with_extension, read, workspace_root};

pub(crate) fn verify_replay(mode: Mode) -> Result<()> {
    let root = workspace_root()?;
    let checks = [
        (
            "workers/projector/src/replay/mod.rs",
            "projector replay module missing",
            Some(Regex::new("ReplayStrategy|start_sequence")?),
        ),
        (
            "workers/projector/src/source.rs",
            "projector replay source must read event_outbox by sequence",
            Some(Regex::new("event_outbox[\\s\\S]*sequence > \\?")?),
        ),
        (
            "workers/projector/src/main.rs",
            "projector must replay outbox and advance checkpoint only after processing",
            Some(Regex::new(
                "replay_outbox[\\s\\S]*process_event[\\s\\S]*checkpoint\\(\\)[\\s\\S]*advance",
            )?),
        ),
        (
            "workers/projector/src/readmodels/mod.rs",
            "projector read model must be rebuildable from replayed events and not authoritative truth",
            Some(Regex::new(
                "counter_projection|Read model|materialized view",
            )?),
        ),
        (
            "workers/projector/README.md",
            "projector README missing replay/rebuild guidance",
            Some(Regex::new("replay|rebuild|read model|replicas=1")?),
        ),
        (
            "services/counter-service/README.md",
            "counter reference missing projection/replay note",
            Some(Regex::new("projection|replay")?),
        ),
        (
            "platform/model/deployables/projector-worker.yaml",
            "projector deployable missing async projection profile",
            Some(Regex::new("async-projection|replicas:\\s*1")?),
        ),
        (
            "verification/golden/README.md",
            "golden README missing replay lane documentation",
            Some(Regex::new("replay")?),
        ),
    ];
    let mut failures = Vec::new();
    for (relative, message, pattern) in checks {
        let path = root.join(relative);
        if !path.exists() {
            failures.push(format!("{relative}: {message}"));
            continue;
        }
        if let Some(pattern) = pattern {
            let content = read(&path)?;
            if !pattern.is_match(&content) {
                failures.push(format!("{relative}: {message}"));
            }
        }
    }
    if failures.is_empty() {
        println!("Replay verification passed");
        return Ok(());
    }
    let mut report = Report::new("verify-replay", mode);
    report.extend(
        failures
            .iter()
            .map(|failure| Issue::warn("replay", failure.clone())),
    );
    report.print();
    report.exit_if_needed();
    Ok(())
}

#[derive(Debug, Deserialize)]
struct CodemapYaml {
    modules: Option<CodemapModules>,
}

#[derive(Debug, Deserialize)]
struct CodemapModules {
    workers: Option<BTreeMap<String, WorkerDeclaration>>,
}

#[derive(Debug, Deserialize)]
struct WorkerDeclaration {
    path: String,
    status: Option<String>,
    notes: Option<String>,
    #[serde(default)]
    must_have: Vec<String>,
}

struct ResilienceCheck {
    worker_name: String,
    strategy: String,
    present: bool,
    evidence: String,
}

pub(crate) fn validate_resilience(mode: Mode) -> Result<()> {
    let root = workspace_root()?;
    let codemap: CodemapYaml = serde_yaml::from_str(&read(root.join("agent/codemap.yml"))?)?;
    let workers = codemap
        .modules
        .and_then(|modules| modules.workers)
        .unwrap_or_default();

    println!("\n=== validate-resilience ({}) ===\n", mode.label());

    let mut issues = Vec::new();
    let mut checks = Vec::new();

    for (worker_name, worker) in workers {
        let worker_path = root.join(&worker.path);
        let src_dir = worker_path.join("src");

        if is_planned_worker(&worker) {
            println!("  - {worker_name} ({}) - planned, skipping", worker.path);
            continue;
        }

        if !worker_path.exists() {
            issues.push(Issue::new(
                issue_severity(mode),
                format!("{worker_name}/(directory)"),
                format!("worker directory does not exist: {}", worker.path),
            ));
            println!("  x {worker_name} ({}) - directory missing", worker.path);
            continue;
        }

        if !src_dir.exists() {
            issues.push(Issue::new(
                issue_severity(mode),
                format!("{worker_name}/(src)"),
                format!("src directory missing for {}", worker.path),
            ));
            println!("  x {worker_name} ({}) - src/ missing", worker.path);
            continue;
        }

        if worker.must_have.is_empty() {
            println!(
                "  - {worker_name} ({}) - no must_have strategies declared",
                worker.path
            );
            continue;
        }

        println!(
            "  -> {worker_name} ({}) - checking {}",
            worker.path,
            worker.must_have.join(", ")
        );

        for strategy in &worker.must_have {
            let (present, evidence) = detect_strategy(&src_dir, strategy)?;
            checks.push(ResilienceCheck {
                worker_name: worker_name.clone(),
                strategy: strategy.clone(),
                present,
                evidence: evidence.clone(),
            });
            if !present {
                issues.push(Issue::new(
                    issue_severity(mode),
                    format!("{worker_name}/{strategy}"),
                    format!("missing required resilience strategy: {strategy} - {evidence}"),
                ));
            }
        }
    }

    println!("\n--- Resilience Strategy Report ---\n");
    let mut by_worker: BTreeMap<String, Vec<&ResilienceCheck>> = BTreeMap::new();
    for check in &checks {
        by_worker
            .entry(check.worker_name.clone())
            .or_default()
            .push(check);
    }
    for (worker_name, worker_checks) in by_worker {
        let marker = if worker_checks.iter().all(|check| check.present) {
            "ok"
        } else {
            "missing"
        };
        println!("  {marker} {worker_name}:");
        for check in worker_checks {
            let status = if check.present { "ok" } else { "missing" };
            println!("    {status} {}: {}", check.strategy, check.evidence);
        }
        println!();
    }

    if issues.is_empty() {
        println!("No resilience issues found");
        return Ok(());
    }

    let mut report = Report::new("validate-resilience", mode);
    report.extend(issues);
    report.print();
    report.exit_if_needed();
    Ok(())
}

fn issue_severity(mode: Mode) -> crate::support::Severity {
    if mode.is_strict() {
        crate::support::Severity::Error
    } else {
        crate::support::Severity::Warn
    }
}

fn is_planned_worker(worker: &WorkerDeclaration) -> bool {
    let status = worker.status.as_deref().unwrap_or_default();
    let notes = worker.notes.as_deref().unwrap_or_default();
    status == "planned" || notes.contains("尚未实现") || notes.contains("占位")
}

fn detect_strategy(src_dir: &Path, strategy: &str) -> Result<(bool, String)> {
    match strategy {
        "dedupe_or_resume_strategy" | "dedupe_or_resume" => {
            let dedupe = detect_strategy(src_dir, "dedupe")?;
            let checkpoint = detect_strategy(src_dir, "checkpoint")?;
            Ok((
                dedupe.0 || checkpoint.0,
                format!(
                    "dedupe: {} {} | checkpoint: {} {}",
                    present_label(dedupe.0),
                    dedupe.1,
                    present_label(checkpoint.0),
                    checkpoint.1
                ),
            ))
        }
        "checkpoint" => detect_with_rules(
            src_dir,
            &[
                Detector::directory("checkpoint", "checkpoint/ module found"),
                Detector::main_rs(
                    Regex::new(r"mod\s+checkpoint")?,
                    "mod checkpoint declared in main.rs",
                ),
                Detector::main_rs(
                    Regex::new(r"checkpoint")?,
                    "checkpoint references found in main.rs",
                ),
                Detector::deep(Regex::new(r"\bcheckpoint\b")?),
            ],
        ),
        "dedupe" => detect_with_rules(
            src_dir,
            &[
                Detector::directory("dedupe", "dedupe/ module found"),
                Detector::main_rs(
                    Regex::new(r"mod\s+dedupe")?,
                    "mod dedupe declared in main.rs",
                ),
                Detector::main_rs(
                    Regex::new(r"\b(dedupe|dedup)\b")?,
                    "dedupe/dedup references found in main.rs",
                ),
                Detector::deep(Regex::new(r"\b(dedupe|dedup)\b")?),
            ],
        ),
        "idempotency" => detect_with_rules(
            src_dir,
            &[
                Detector::directory("idempotency", "idempotency/ module found"),
                Detector::file("idempotent.rs", "idempotent.rs found"),
                Detector::main_rs(
                    Regex::new(r"mod\s+idempoten")?,
                    "idempotency module declared in main.rs",
                ),
                Detector::main_rs(
                    Regex::new(r"\b(idempoten|idempotency_key|idempotent)\b")?,
                    "idempotency references found in main.rs",
                ),
                Detector::deep(Regex::new(r"\b(idempoten|idempotency_key|idempotent)\b")?),
            ],
        ),
        "retry_policy" => detect_with_rules(
            src_dir,
            &[
                Detector::directory("retry", "retry/ module found"),
                Detector::main_rs(Regex::new(r"mod\s+retry")?, "mod retry declared in main.rs"),
                Detector::main_rs(
                    Regex::new(r"\b(retry_policy|retry_count|ExponentialBackoff|retry\b)")?,
                    "retry references found in main.rs",
                ),
                Detector::deep(Regex::new(
                    r"\b(retry_policy|retry_count|ExponentialBackoff|retry\b)",
                )?),
            ],
        ),
        "rebuildable_read_model" => detect_with_rules(
            src_dir,
            &[
                Detector::directory("readmodels", "readmodels/ module found"),
                Detector::deep(Regex::new(
                    r"\b(counter_projection|ReadModel|read model|materialized view)\b",
                )?),
                Detector::deep(Regex::new(r"\b(init|CREATE TABLE IF NOT EXISTS)\b")?),
            ],
        ),
        "replay_strategy" => detect_with_rules(
            src_dir,
            &[
                Detector::directory("replay", "replay/ module found"),
                Detector::main_rs(
                    Regex::new(r"mod\s+replay")?,
                    "mod replay declared in main.rs",
                ),
                Detector::main_rs(
                    Regex::new(r"\breplay")?,
                    "replay references found in main.rs",
                ),
                Detector::deep(Regex::new(r"\breplay\b")?),
            ],
        ),
        "conflict_strategy" => detect_with_rules(
            src_dir,
            &[
                Detector::directory("conflict", "conflict/ module found"),
                Detector::main_rs(
                    Regex::new(r"mod\s+conflict")?,
                    "mod conflict declared in main.rs",
                ),
                Detector::main_rs(
                    Regex::new(r"\bconflict")?,
                    "conflict references found in main.rs",
                ),
                Detector::deep(Regex::new(r"\bconflict\b")?),
            ],
        ),
        "compensation_strategy" => detect_with_rules(
            src_dir,
            &[
                Detector::directory("compensation", "compensation/ module found"),
                Detector::file("saga.rs", "saga.rs found"),
                Detector::main_rs(
                    Regex::new(r"mod\s+compensation")?,
                    "mod compensation declared in main.rs",
                ),
                Detector::main_rs(
                    Regex::new(r"\b(compensation|compensate|saga)\b")?,
                    "compensation/saga references found in main.rs",
                ),
                Detector::deep(Regex::new(r"\b(compensation|compensate|saga)\b")?),
            ],
        ),
        other => Ok((false, format!("unknown strategy: {other}"))),
    }
}

enum Detector {
    Directory {
        name: &'static str,
        evidence: &'static str,
    },
    File {
        name: &'static str,
        evidence: &'static str,
    },
    MainRs {
        pattern: Regex,
        evidence: &'static str,
    },
    Deep {
        pattern: Regex,
    },
}

impl Detector {
    fn directory(name: &'static str, evidence: &'static str) -> Self {
        Self::Directory { name, evidence }
    }

    fn file(name: &'static str, evidence: &'static str) -> Self {
        Self::File { name, evidence }
    }

    fn main_rs(pattern: Regex, evidence: &'static str) -> Self {
        Self::MainRs { pattern, evidence }
    }

    fn deep(pattern: Regex) -> Self {
        Self::Deep { pattern }
    }
}

fn detect_with_rules(src_dir: &Path, detectors: &[Detector]) -> Result<(bool, String)> {
    for detector in detectors {
        match detector {
            Detector::Directory { name, evidence } => {
                let directory = src_dir.join(name);
                if directory.exists() && has_rust_file(&directory)? {
                    return Ok((true, (*evidence).to_string()));
                }
            }
            Detector::File { name, evidence } => {
                if src_dir.join(name).exists() {
                    return Ok((true, (*evidence).to_string()));
                }
            }
            Detector::MainRs { pattern, evidence } => {
                let main_rs = src_dir.join("main.rs");
                if main_rs.exists() && pattern.is_match(&read(&main_rs)?) {
                    return Ok((true, (*evidence).to_string()));
                }
            }
            Detector::Deep { pattern } => {
                if let Some(evidence) = scan_all_rust_files(src_dir, pattern)? {
                    return Ok((true, format!("deep scan: {evidence}")));
                }
            }
        }
    }
    Ok((
        false,
        "not found in module structure or source content".to_string(),
    ))
}

fn has_rust_file(directory: &Path) -> Result<bool> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|ext| ext == "rs") {
            return Ok(true);
        }
    }
    Ok(false)
}

fn scan_all_rust_files(src_dir: &Path, pattern: &Regex) -> Result<Option<String>> {
    let mut matches = Vec::new();
    for file in collect_files_with_extension(src_dir, "rs") {
        if pattern.is_match(&read(&file)?) {
            let relative = file
                .strip_prefix(src_dir)?
                .to_string_lossy()
                .replace('\\', "/");
            matches.push(relative);
        }
    }
    if matches.is_empty() {
        Ok(None)
    } else {
        Ok(Some(format!(
            "{} Rust file(s) with matches: {}",
            matches.len(),
            matches.join(", ")
        )))
    }
}

fn present_label(present: bool) -> &'static str {
    if present { "ok" } else { "missing" }
}
