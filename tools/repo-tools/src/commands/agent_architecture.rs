use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};

use crate::support::{Issue, Mode, Report, normalize_slashes, read, workspace_root};

pub(crate) fn validate_agent_architecture(mode: Mode) -> Result<()> {
    let root = workspace_root()?;
    let mut issues = Vec::new();

    let required_architecture = [
        "repository-ontology.yml",
        "directory-grammar.yml",
        "naming-conventions.yml",
        "gate-taxonomy.yml",
        "evidence-taxonomy.yml",
        "entropy-guardrails.yml",
    ];
    let architecture_dir = root.join("agent/architecture");
    for file_name in required_architecture {
        let path = architecture_dir.join(file_name);
        if !path.exists() {
            issues.push(Issue::error(
                format!("agent/architecture/{file_name}"),
                "required architecture control file is missing",
            ));
            continue;
        }
        validate_agent_yaml_file(&root, &path, "architecture", &mut issues)?;
    }

    let forbidden_global_refactor = architecture_dir.join("refactor-protocol.yml");
    if forbidden_global_refactor.exists() {
        issues.push(Issue::error(
            "agent/architecture/refactor-protocol.yml",
            "refactor protocol must be task-scoped under agent/task-profiles/refactor.yml",
        ));
    }

    let refactor_profile = root.join("agent/task-profiles/refactor.yml");
    if !refactor_profile.exists() {
        issues.push(Issue::error(
            "agent/task-profiles/refactor.yml",
            "required refactor task profile is missing",
        ));
    } else {
        validate_agent_yaml_file(&root, &refactor_profile, "task-profile", &mut issues)?;
        let profile: serde_yaml::Value = serde_yaml::from_str(&read(&refactor_profile)?)?;
        if profile.get("activation_triggers").is_none() {
            issues.push(Issue::error(
                "agent/task-profiles/refactor.yml",
                "task profile must declare activation_triggers",
            ));
        }
    }

    for manifest in [
        "agent/codemap.yml",
        "agent/manifests/routing-rules.yml",
        "agent/manifests/gate-matrix.yml",
    ] {
        let path = root.join(manifest);
        if !path.exists() {
            issues.push(Issue::error(manifest, "required agent manifest is missing"));
            continue;
        }
        let _: serde_yaml::Value = serde_yaml::from_str(&read(&path)?)
            .with_context(|| format!("failed to parse {manifest}"))?;
    }
    validate_gate_matrix_references(&root, &mut issues)?;

    let mut report = Report::new("validate-agent-architecture", mode);
    report.extend(issues);
    report.print();
    if report.is_empty() {
        println!("Agent architecture control plane is structurally valid");
        return Ok(());
    }
    report.exit_if_needed();
    Ok(())
}

fn validate_gate_matrix_references(root: &Path, issues: &mut Vec<Issue>) -> Result<()> {
    let relative = "agent/manifests/gate-matrix.yml";
    let path = root.join(relative);
    let value: serde_yaml::Value = serde_yaml::from_str(&read(&path)?)
        .with_context(|| format!("failed to parse {relative}"))?;
    let Some(mapping) = value.as_mapping() else {
        issues.push(Issue::error(relative, "YAML root must be a mapping"));
        return Ok(());
    };

    let gate_names = mapping
        .get(serde_yaml::Value::from("gates"))
        .and_then(serde_yaml::Value::as_mapping)
        .map(|gates| {
            gates
                .keys()
                .filter_map(serde_yaml::Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();

    if gate_names.is_empty() {
        issues.push(Issue::error(
            relative,
            "gate matrix must declare at least one gate under gates",
        ));
        return Ok(());
    }

    if let Some(path_rules) = mapping
        .get(serde_yaml::Value::from("path_rules"))
        .and_then(serde_yaml::Value::as_sequence)
    {
        for (index, rule) in path_rules.iter().enumerate() {
            for key in ["recommended", "required"] {
                check_gate_reference_list(
                    relative,
                    &format!("path_rules[{index}].{key}"),
                    rule.get(key),
                    &gate_names,
                    issues,
                );
            }
        }
    }

    if let Some(risk_rules) = mapping
        .get(serde_yaml::Value::from("risk_rules"))
        .and_then(serde_yaml::Value::as_mapping)
    {
        for (risk_name, rule) in risk_rules {
            let risk_name = risk_name.as_str().unwrap_or("<unknown>");
            for key in ["add_recommended", "add_required"] {
                check_gate_reference_list(
                    relative,
                    &format!("risk_rules.{risk_name}.{key}"),
                    rule.get(key),
                    &gate_names,
                    issues,
                );
            }
        }
    }

    Ok(())
}

fn check_gate_reference_list(
    file: &str,
    scope: &str,
    value: Option<&serde_yaml::Value>,
    gate_names: &BTreeSet<String>,
    issues: &mut Vec<Issue>,
) {
    for gate in yaml_string_values(value) {
        if !gate_names.contains(&gate) {
            issues.push(Issue::error(
                file,
                format!("{scope} references undefined gate {gate:?}"),
            ));
        }
    }
}

fn yaml_string_values(value: Option<&serde_yaml::Value>) -> Vec<String> {
    match value {
        Some(serde_yaml::Value::Sequence(sequence)) => sequence
            .iter()
            .filter_map(serde_yaml::Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        Some(serde_yaml::Value::String(value)) => vec![value.clone()],
        _ => Vec::new(),
    }
}

fn validate_agent_yaml_file(
    root: &Path,
    path: &Path,
    file_kind: &str,
    issues: &mut Vec<Issue>,
) -> Result<()> {
    let relative = normalize_slashes(path.strip_prefix(root)?);
    let value: serde_yaml::Value = serde_yaml::from_str(&read(path)?)
        .with_context(|| format!("failed to parse {relative}"))?;
    let Some(mapping) = value.as_mapping() else {
        issues.push(Issue::error(relative, "YAML root must be a mapping"));
        return Ok(());
    };
    for key in ["version", "status", "purpose"] {
        if !mapping.contains_key(serde_yaml::Value::from(key)) {
            issues.push(Issue::error(
                relative.clone(),
                format!("{file_kind} YAML missing required key: {key}"),
            ));
        }
    }
    let status = mapping
        .get(serde_yaml::Value::from("status"))
        .and_then(serde_yaml::Value::as_str);
    if let Some(status) = status {
        if !matches!(status, "active" | "draft" | "deprecated") {
            issues.push(Issue::error(
                relative,
                format!("invalid status {status:?}; expected active, draft, or deprecated"),
            ));
        }
    }
    Ok(())
}
