use std::collections::BTreeMap;

use anyhow::Result;

use crate::core::manifest::cargo_metadata;
use crate::support::{
    Issue, Mode, Report, collect_files_named, except_path, normalize_slashes, pattern_matches,
    read, same_module, workspace_root,
};

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
    for scope in ["packages", "platform", "servers", "services", "workers"] {
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
                            issues.push((
                                mode.is_strict(),
                                source_path.clone(),
                                format!("depends on path outside allowlist: {dependency} (rule: {rule_name})"),
                            ));
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
