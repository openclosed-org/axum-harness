#![deny(unused_imports, unused_variables)]

use anyhow::{Context, Result};
use clap::Parser;
use jsonschema::Validator;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "platform-validator")]
#[command(about = "Validate platform models against JSON schemas")]
struct Args {
    /// Path to platform directory
    #[arg(short, long, default_value = "platform")]
    platform_dir: PathBuf,

    /// Output format
    #[arg(short, long, default_value = "text")]
    format: OutputFormat,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Serialize, Deserialize)]
struct ValidationResult {
    file: String,
    schema: String,
    valid: bool,
    errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Summary {
    total: usize,
    passed: usize,
    failed: usize,
    warnings: usize,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let platform_dir = fs::canonicalize(&args.platform_dir).with_context(|| {
        format!(
            "Platform directory not found: {}",
            args.platform_dir.display()
        )
    })?;

    info!("Validating platform models in {}", platform_dir.display());

    let schema_dir = platform_dir.join("schema");
    let model_dir = platform_dir.join("model");

    if !schema_dir.exists() {
        anyhow::bail!("Schema directory not found: {}", schema_dir.display());
    }
    if !model_dir.exists() {
        anyhow::bail!("Model directory not found: {}", model_dir.display());
    }

    let mut results = Vec::new();
    let mut warnings = 0;

    // Define model type to schema file mapping
    let model_schema_map = [
        ("capabilities", "capability.schema.json"),
        ("services", "service-metadata.schema.json"),
        ("deployables", "deployable.schema.json"),
        ("resources", "resource.schema.json"),
        ("workflows", "workflow.schema.json"),
        ("topologies", "topology.schema.json"),
        ("policies", "policy.schema.json"),
    ];

    for (model_type, schema_file) in &model_schema_map {
        let schema_path = schema_dir.join(schema_file);
        if !schema_path.exists() {
            warn!(
                "Schema not found: {} — skipping {}",
                schema_path.display(),
                model_type
            );
            warnings += 1;
            continue;
        }

        let schema_content = fs::read_to_string(&schema_path)
            .with_context(|| format!("Failed to read schema: {}", schema_path.display()))?;

        let schema: serde_json::Value = serde_json::from_str(&schema_content)
            .with_context(|| format!("Invalid JSON in schema: {}", schema_path.display()))?;

        let compiled_schema = Validator::new(&schema)
            .with_context(|| format!("Invalid schema structure: {}", schema_file))?;

        let model_type_dir = model_dir.join(model_type);
        if !model_type_dir.exists() {
            warn!(
                "Model directory not found: {} — skipping",
                model_type_dir.display()
            );
            warnings += 1;
            continue;
        }

        // Find all YAML files in this model type directory
        let yaml_pattern = format!("{}/*.yaml", model_type_dir.display());
        let yaml_files: Vec<PathBuf> = glob::glob(&yaml_pattern)
            .with_context(|| format!("Invalid glob pattern: {}", yaml_pattern))?
            .filter_map(|p| p.ok())
            .collect();

        for yaml_file in &yaml_files {
            let content = fs::read_to_string(yaml_file)
                .with_context(|| format!("Failed to read: {}", yaml_file.display()))?;

            let yaml_value: serde_json::Value = serde_yaml::from_str(&content)
                .with_context(|| format!("Invalid YAML in: {}", yaml_file.display()))?;

            let mut errors = Vec::new();

            // Validate using the Validator API - is_valid returns bool
            if !compiled_schema.is_valid(&yaml_value) {
                // Collect errors manually if needed
                errors.push("Validation failed".to_string());
            }

            let valid = errors.is_empty();
            let file_name = yaml_file
                .strip_prefix(&platform_dir)
                .unwrap_or(yaml_file)
                .to_string_lossy()
                .to_string();

            results.push(ValidationResult {
                file: file_name,
                schema: schema_file.to_string(),
                valid,
                errors,
            });
        }
    }

    // Additional checks: cross-reference validation
    info!("Running cross-reference checks...");
    warnings += check_service_deployable_refs(&platform_dir, &mut results);
    warnings += check_resource_refs(&platform_dir, &mut results);
    check_capability_naming(&platform_dir, &mut results);
    check_deployable_status_consistency(&platform_dir, &mut results);

    // Print results
    let passed = results.iter().filter(|r| r.valid).count();
    let failed = results.iter().filter(|r| !r.valid).count();

    match args.format {
        OutputFormat::Text => print_text_report(&results, passed, failed, warnings),
        OutputFormat::Json => print_json_report(&results, passed, failed, warnings),
    }

    if failed > 0 {
        anyhow::bail!("Validation failed: {} errors", failed);
    }

    info!("✅ All models valid!");
    Ok(())
}

fn check_service_deployable_refs(platform_dir: &Path, _results: &mut [ValidationResult]) -> usize {
    let mut warnings = 0;

    // Load all service models
    let services_dir = platform_dir.join("model/services");
    if !services_dir.exists() {
        return 0;
    }

    // Load all deployable models to check references
    let deployables_dir = platform_dir.join("model/deployables");
    if !deployables_dir.exists() {
        return 0;
    }

    let mut deployable_names = Vec::new();
    if let Ok(entries) = fs::read_dir(&deployables_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("yaml")
                && let Ok(content) = fs::read_to_string(entry.path())
                && let Ok(yaml) = serde_yaml::from_str::<serde_json::Value>(&content)
                && let Some(name) = yaml.get("name").and_then(|v| v.as_str())
            {
                deployable_names.push(name.to_string());
            }
        }
    }

    // Check service -> deployable references
    if let Ok(entries) = fs::read_dir(&services_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("yaml")
                && let Ok(content) = fs::read_to_string(entry.path())
                && let Ok(yaml) = serde_yaml::from_str::<serde_json::Value>(&content)
                && let Some(deployable) = yaml.get("deployable").and_then(|v| v.as_str())
                && !deployable_names.contains(&deployable.to_string())
            {
                warnings += 1;
                warn!(
                    "Service {} references non-existent deployable: {}",
                    entry.path().display(),
                    deployable
                );
            }
        }
    }

    warnings
}

fn check_resource_refs(platform_dir: &Path, _results: &mut [ValidationResult]) -> usize {
    let mut warnings = 0;

    // Load all resource names
    let resources_dir = platform_dir.join("model/resources");
    if !resources_dir.exists() {
        return 0;
    }

    let mut resource_names = Vec::new();
    if let Ok(entries) = fs::read_dir(&resources_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("yaml")
                && let Ok(content) = fs::read_to_string(entry.path())
                && let Ok(yaml) = serde_yaml::from_str::<serde_json::Value>(&content)
                && let Some(name) = yaml.get("name").and_then(|v| v.as_str())
            {
                resource_names.push(name.to_string());
            }
        }
    }

    // Check deployable -> resource references
    let deployables_dir = platform_dir.join("model/deployables");
    if !deployables_dir.exists() {
        return 0;
    }

    if let Ok(entries) = fs::read_dir(&deployables_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("yaml")
                && let Ok(content) = fs::read_to_string(entry.path())
                && let Ok(yaml) = serde_yaml::from_str::<serde_json::Value>(&content)
                && let Some(resources) = yaml.get("resources").and_then(|v| v.as_array())
            {
                for resource in resources {
                    if let Some(res_name) = resource.as_str()
                        && !resource_names.contains(&res_name.to_string())
                    {
                        warnings += 1;
                        warn!(
                            "Deployable {} references non-existent resource: {}",
                            entry.path().display(),
                            res_name
                        );
                    }
                }
            }
        }
    }

    warnings
}

fn check_deployable_status_consistency(platform_dir: &Path, results: &mut Vec<ValidationResult>) {
    let deployables_dir = platform_dir.join("model/deployables");
    if !deployables_dir.exists() {
        return;
    }

    let yaml_pattern = format!("{}/*.yaml", deployables_dir.display());
    let Ok(yaml_files) = glob::glob(&yaml_pattern) else {
        return;
    };

    for yaml_file in yaml_files.filter_map(|path| path.ok()) {
        let file_name = yaml_file
            .strip_prefix(platform_dir)
            .unwrap_or(&yaml_file)
            .to_string_lossy()
            .to_string();

        let mut errors = Vec::new();
        let content = match fs::read_to_string(&yaml_file) {
            Ok(content) => content,
            Err(error) => {
                errors.push(format!("Failed to read deployable model: {error}"));
                results.push(ValidationResult {
                    file: file_name,
                    schema: "deployable-status-consistency".to_string(),
                    valid: false,
                    errors,
                });
                continue;
            }
        };

        let yaml: serde_json::Value = match serde_yaml::from_str(&content) {
            Ok(yaml) => yaml,
            Err(error) => {
                errors.push(format!("Invalid deployable YAML: {error}"));
                results.push(ValidationResult {
                    file: file_name,
                    schema: "deployable-status-consistency".to_string(),
                    valid: false,
                    errors,
                });
                continue;
            }
        };

        let name = yaml.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let current_status = yaml.get("current_status").and_then(|v| v.as_str());
        let target_status = yaml.get("target_status").and_then(|v| v.as_str());
        let independent_deploy = yaml
            .get("independent_deploy")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let embedded_in = yaml
            .get("embedded_in")
            .and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(|v| v.as_str()).count())
            .unwrap_or(0);
        let entry_point = yaml.get("entry_point").and_then(|v| v.as_str());
        let package = yaml.get("package").and_then(|v| v.as_str());
        let planned_capabilities = yaml
            .get("planned_capabilities")
            .and_then(|v| v.as_array())
            .map(|items| items.len())
            .unwrap_or(0);

        if current_status.is_none() {
            errors.push("deployables must declare current_status".to_string());
        }

        if target_status.is_none() {
            errors.push("deployables must declare target_status".to_string());
        }

        if yaml.get("planned_capabilities").is_none() {
            errors.push(
                "deployables must declare planned_capabilities (use [] when empty)".to_string(),
            );
        }

        if matches!(current_status, Some("embedded")) && embedded_in == 0 {
            errors.push("current_status=embedded requires embedded_in".to_string());
        }

        if matches!(current_status, Some("embedded")) && independent_deploy {
            errors.push("current_status=embedded cannot set independent_deploy=true".to_string());
        }

        if matches!(current_status, Some("planned") | Some("stub")) && independent_deploy {
            errors.push("planned/stub deployables cannot set independent_deploy=true".to_string());
        }

        if matches!(target_status, Some("independent"))
            && matches!(
                current_status,
                Some("embedded") | Some("planned") | Some("stub")
            )
            && planned_capabilities == 0
        {
            errors
                .push("future independent deployables must list planned_capabilities".to_string());
        }

        if matches!(current_status, Some("implemented")) && !independent_deploy {
            errors.push(
                "current_status=implemented requires independent_deploy=true for real runnable binaries"
                    .to_string(),
            );
        }

        if independent_deploy && entry_point.is_none() {
            errors.push("independent_deploy=true requires entry_point".to_string());
        }

        if independent_deploy && package.is_none() {
            errors.push("independent_deploy=true requires package".to_string());
        }

        if independent_deploy
            && let Some(package) = package
            && !package_name_exists(platform_dir, package)
        {
            errors.push(format!(
                "independent_deploy=true references missing package '{package}'"
            ));
        }

        if independent_deploy
            && let Some(entry_point) = entry_point
            && !platform_dir
                .parent()
                .unwrap_or(platform_dir)
                .join(entry_point)
                .exists()
        {
            errors.push(format!(
                "independent_deploy=true references missing entry_point '{entry_point}'"
            ));
        }

        if name == "counter-service" && !matches!(current_status, Some("embedded")) {
            errors.push(
                "counter-service must declare current_status=embedded until it has its own binary"
                    .to_string(),
            );
        }

        if errors.is_empty() {
            continue;
        }

        results.push(ValidationResult {
            file: file_name,
            schema: "deployable-status-consistency".to_string(),
            valid: false,
            errors,
        });
    }
}

fn check_capability_naming(platform_dir: &Path, results: &mut Vec<ValidationResult>) {
    let capabilities = load_capabilities(platform_dir, results);
    if capabilities.is_empty() {
        return;
    }

    for capability in capabilities.values() {
        let mut errors = Vec::new();
        let provider_prefix = format!("{}.", capability.key);

        for provider in &capability.providers {
            if !provider.key.starts_with(&provider_prefix) {
                errors.push(format!(
                    "provider key '{}' must start with capability prefix '{}'",
                    provider.key, provider_prefix
                ));
            }
        }

        for adapter in &capability.adapters {
            if !adapter.key.starts_with(&provider_prefix) {
                errors.push(format!(
                    "adapter key '{}' must start with capability prefix '{}'",
                    adapter.key, provider_prefix
                ));
            }
        }

        for resource in &capability.resource_entities {
            if contains_reserved_resource_word(&resource.key) {
                errors.push(format!(
                    "resource entity '{}' must not encode state/topology words such as shared, external, local, mock, real, or distributed",
                    resource.key
                ));
            }
        }

        let provider_keys = capability
            .providers
            .iter()
            .map(|provider| provider.key.as_str())
            .collect::<BTreeSet<_>>();
        for (state_name, state) in &capability.states {
            if let Some(default_provider) = &state.default_provider
                && !provider_keys.contains(default_provider.as_str())
            {
                errors.push(format!(
                    "state '{}' references unknown default_provider '{}'",
                    state_name, default_provider
                ));
            }
        }

        if !errors.is_empty() {
            results.push(ValidationResult {
                file: capability.file.clone(),
                schema: "capability-naming".to_string(),
                valid: false,
                errors,
            });
        }
    }

    check_topology_capability_bindings(platform_dir, results, &capabilities);
}

fn check_topology_capability_bindings(
    platform_dir: &Path,
    results: &mut Vec<ValidationResult>,
    capabilities: &BTreeMap<String, CapabilityModel>,
) {
    let topologies_dir = platform_dir.join("model/topologies");
    if !topologies_dir.exists() {
        return;
    }

    let yaml_pattern = format!("{}/*.yaml", topologies_dir.display());
    let Ok(yaml_files) = glob::glob(&yaml_pattern) else {
        return;
    };

    for yaml_file in yaml_files.filter_map(|path| path.ok()) {
        let file_name = yaml_file
            .strip_prefix(platform_dir)
            .unwrap_or(&yaml_file)
            .to_string_lossy()
            .to_string();
        let mut errors = Vec::new();

        let content = match fs::read_to_string(&yaml_file) {
            Ok(content) => content,
            Err(error) => {
                results.push(ValidationResult {
                    file: file_name,
                    schema: "capability-bindings".to_string(),
                    valid: false,
                    errors: vec![format!("Failed to read topology model: {error}")],
                });
                continue;
            }
        };

        let yaml: serde_json::Value = match serde_yaml::from_str(&content) {
            Ok(yaml) => yaml,
            Err(error) => {
                results.push(ValidationResult {
                    file: file_name,
                    schema: "capability-bindings".to_string(),
                    valid: false,
                    errors: vec![format!("Invalid topology YAML: {error}")],
                });
                continue;
            }
        };

        let Some(bindings) = yaml
            .get("capability_bindings")
            .and_then(|value| value.as_object())
        else {
            continue;
        };

        for (capability_key, binding) in bindings {
            let Some(capability) = capabilities.get(capability_key) else {
                errors.push(format!(
                    "capability_bindings references unknown capability '{}'",
                    capability_key
                ));
                continue;
            };

            let state_name = binding.get("state").and_then(|value| value.as_str());
            let provider = binding.get("provider").and_then(|value| value.as_str());
            let resource = binding.get("resource").and_then(|value| value.as_str());
            let adapter = binding.get("adapter").and_then(|value| value.as_str());
            let secret_binding = binding
                .get("secret_binding")
                .and_then(|value| value.as_str());

            if let Some(state_name) = state_name {
                match capability.states.get(state_name) {
                    Some(state) if !state.supported => {
                        errors.push(format!(
                            "capability '{}' binding selects unsupported state '{}'",
                            capability_key, state_name
                        ));
                    }
                    Some(_) => {}
                    None => {
                        errors.push(format!(
                            "capability '{}' binding references unknown state '{}'",
                            capability_key, state_name
                        ));
                    }
                }
            }

            if let Some(provider) = provider {
                let expected_prefix = format!("{}.", capability_key);
                if !provider.starts_with(&expected_prefix) {
                    errors.push(format!(
                        "provider '{}' must start with capability prefix '{}'",
                        provider, expected_prefix
                    ));
                }
                if !capability
                    .providers
                    .iter()
                    .any(|known_provider| known_provider.key == provider)
                {
                    errors.push(format!(
                        "provider '{}' is not declared by capability '{}'",
                        provider, capability_key
                    ));
                }
            }

            if let Some(adapter) = adapter {
                let expected_prefix = format!("{}.", capability_key);
                if !adapter.starts_with(&expected_prefix) {
                    errors.push(format!(
                        "adapter '{}' must start with capability prefix '{}'",
                        adapter, expected_prefix
                    ));
                }
                if !capability
                    .adapters
                    .iter()
                    .any(|known_adapter| known_adapter.key == adapter)
                {
                    errors.push(format!(
                        "adapter '{}' is not declared by capability '{}'",
                        adapter, capability_key
                    ));
                }
            }

            if let Some(resource) = resource {
                if contains_reserved_resource_word(resource) {
                    errors.push(format!(
                        "resource '{}' must not encode state/topology words such as shared, external, local, mock, real, or distributed",
                        resource
                    ));
                }
                if !capability
                    .resource_entities
                    .iter()
                    .any(|known_resource| known_resource.key == resource)
                {
                    errors.push(format!(
                        "resource '{}' is not declared by capability '{}'",
                        resource, capability_key
                    ));
                }
            }

            if let Some(secret_binding) = secret_binding
                && let Some(reason) = secret_binding_name_error(
                    secret_binding,
                    yaml.get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or(""),
                    provider,
                )
            {
                errors.push(format!(
                    "secret_binding '{}' must not encode state/topology/provider facts: {}",
                    secret_binding, reason
                ));
            }
        }

        if !errors.is_empty() {
            results.push(ValidationResult {
                file: file_name,
                schema: "capability-bindings".to_string(),
                valid: false,
                errors,
            });
        }
    }
}

fn load_capabilities(
    platform_dir: &Path,
    results: &mut Vec<ValidationResult>,
) -> BTreeMap<String, CapabilityModel> {
    let capabilities_dir = platform_dir.join("model/capabilities");
    let mut capabilities = BTreeMap::new();
    if !capabilities_dir.exists() {
        return capabilities;
    }

    let yaml_pattern = format!("{}/*.yaml", capabilities_dir.display());
    let Ok(yaml_files) = glob::glob(&yaml_pattern) else {
        return capabilities;
    };

    for yaml_file in yaml_files.filter_map(|path| path.ok()) {
        let file_name = yaml_file
            .strip_prefix(platform_dir)
            .unwrap_or(&yaml_file)
            .to_string_lossy()
            .to_string();
        let content = match fs::read_to_string(&yaml_file) {
            Ok(content) => content,
            Err(error) => {
                results.push(ValidationResult {
                    file: file_name,
                    schema: "capability-naming".to_string(),
                    valid: false,
                    errors: vec![format!("Failed to read capability model: {error}")],
                });
                continue;
            }
        };

        let mut capability: CapabilityModel = match serde_yaml::from_str(&content) {
            Ok(capability) => capability,
            Err(error) => {
                results.push(ValidationResult {
                    file: file_name,
                    schema: "capability-naming".to_string(),
                    valid: false,
                    errors: vec![format!("Invalid capability YAML: {error}")],
                });
                continue;
            }
        };
        capability.file = file_name;
        capabilities.insert(capability.key.clone(), capability);
    }

    capabilities
}

fn contains_reserved_resource_word(value: &str) -> bool {
    value.split(['-', '_']).any(|part| {
        matches!(
            part,
            "shared" | "external" | "local" | "mock" | "real" | "distributed"
        )
    })
}

fn secret_binding_name_error(
    secret_binding: &str,
    topology_name: &str,
    provider: Option<&str>,
) -> Option<String> {
    let secret_parts = normalized_token_parts(secret_binding).collect::<BTreeSet<_>>();
    if secret_parts
        .iter()
        .any(|part| is_state_like_token(part.as_str()))
    {
        return Some("state-like token is part of stable secret identity".to_string());
    }

    if !topology_name.is_empty() {
        let topology_parts = normalized_token_parts(topology_name).collect::<BTreeSet<_>>();
        if let Some(part) = topology_parts.intersection(&secret_parts).next() {
            return Some(format!(
                "topology token '{part}' is part of stable secret identity"
            ));
        }
    }

    if let Some(provider) = provider {
        let provider_parts = normalized_token_parts(provider).collect::<BTreeSet<_>>();
        if let Some(part) = provider_parts.intersection(&secret_parts).next() {
            return Some(format!(
                "provider token '{part}' is part of stable secret identity"
            ));
        }
    }

    None
}

fn token_parts(value: &str) -> impl Iterator<Item = &str> {
    value.split(['-', '_', '.']).filter(|part| !part.is_empty())
}

fn normalized_token_parts(value: &str) -> impl Iterator<Item = String> + '_ {
    token_parts(value).map(str::to_ascii_lowercase)
}

fn is_state_like_token(value: &str) -> bool {
    matches!(
        value,
        "disabled" | "local" | "mock" | "real" | "external" | "single" | "node" | "distributed"
    )
}

#[derive(Debug, Deserialize)]
struct CapabilityModel {
    key: String,
    states: BTreeMap<String, CapabilityState>,
    providers: Vec<CapabilityProvider>,
    #[serde(default)]
    adapters: Vec<CapabilityAdapter>,
    #[serde(default)]
    resource_entities: Vec<CapabilityResourceEntity>,
    #[serde(skip)]
    file: String,
}

#[derive(Debug, Deserialize)]
struct CapabilityState {
    supported: bool,
    default_provider: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CapabilityProvider {
    key: String,
}

#[derive(Debug, Deserialize)]
struct CapabilityAdapter {
    key: String,
}

#[derive(Debug, Deserialize)]
struct CapabilityResourceEntity {
    key: String,
}

fn package_name_exists(platform_dir: &Path, package_name: &str) -> bool {
    let repo_root = platform_dir.parent().unwrap_or(platform_dir);
    let cargo_pattern = format!("{}/**/Cargo.toml", repo_root.display());
    let Ok(cargo_files) = glob::glob(&cargo_pattern) else {
        return false;
    };

    for cargo_file in cargo_files.filter_map(|path| path.ok()) {
        let Ok(content) = fs::read_to_string(cargo_file) else {
            continue;
        };
        if content
            .lines()
            .any(|line| line.trim() == format!("name = \"{package_name}\""))
        {
            return true;
        }
    }

    false
}

fn print_text_report(results: &[ValidationResult], passed: usize, failed: usize, warnings: usize) {
    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║          Platform Model Validation Report               ║");
    println!("╠══════════════════════════════════════════════════════════╣");

    for result in results {
        let status = if result.valid { "✅" } else { "❌" };
        println!("{} {} ({})", status, result.file, result.schema);

        if !result.valid {
            for error in &result.errors {
                println!("   └─ {}", error);
            }
        }
    }

    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║ Summary:                                                 ║");
    println!("║   Passed:   {:<45} ║", passed);
    println!("║   Failed:   {:<45} ║", failed);
    println!("║   Warnings: {:<45} ║", warnings);
    println!("╚══════════════════════════════════════════════════════════╝\n");
}

fn print_json_report(results: &[ValidationResult], passed: usize, failed: usize, warnings: usize) {
    let report = serde_json::json!({
        "results": results,
        "summary": {
            "total": results.len(),
            "passed": passed,
            "failed": failed,
            "warnings": warnings
        }
    });

    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_binding_name_rejects_state_topology_and_provider_tokens() {
        assert!(secret_binding_name_error("billing-local-secret", "single-vps", None).is_some());
        assert!(secret_binding_name_error("billing-LOCAL-secret", "single-vps", None).is_some());
        assert!(secret_binding_name_error("web-bff-single-secret", "single-vps", None).is_some());
        assert!(
            secret_binding_name_error("creem-webhook-secret", "single-vps", Some("billing.creem"))
                .is_some()
        );
    }

    #[test]
    fn secret_binding_name_allows_stable_entity_identity() {
        assert!(
            secret_binding_name_error("web-bff", "single-vps", Some("billing.creem")).is_none()
        );
        assert!(
            secret_binding_name_error(
                "counter-db-credentials",
                "single-vps",
                Some("database.turso-cloud")
            )
            .is_none()
        );
    }
}
