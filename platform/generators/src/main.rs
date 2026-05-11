use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "platform-generator")]
#[command(about = "Generate platform catalog from models")]
struct Args {
    /// Path to platform directory
    #[arg(short, long, default_value = "platform")]
    platform_dir: PathBuf,

    /// Output directory for generated catalog
    #[arg(short, long, default_value = "platform/catalog")]
    output_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct ServiceModel {
    name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    domain: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    description: Option<String>,
    status: Option<String>,
    deployable: Option<String>,
    #[serde(default)]
    ports: Vec<serde_json::Value>,
    #[serde(default)]
    events: Vec<serde_json::Value>,
    dependencies: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeployableModel {
    name: String,
    #[serde(rename = "type")]
    deployable_type: String,
    version: String,
    description: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    current_status: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    target_status: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    planned_capabilities: Vec<String>,
    services: Option<Vec<String>>,
    resources: Option<Vec<String>>,
    ports: Option<Vec<u16>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResourceModel {
    name: String,
    #[serde(rename = "type")]
    resource_type: String,
    description: Option<String>,
    technology: Option<String>,
    required_by: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TopologyModel {
    name: String,
    version: String,
    description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CapabilityModel {
    key: String,
    kind: String,
    description: Option<String>,
    states: serde_json::Value,
    providers: Vec<serde_json::Value>,
    #[serde(default)]
    adapters: Vec<serde_json::Value>,
    #[serde(default)]
    resource_entities: Vec<serde_json::Value>,
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

    let output_dir = &args.output_dir;
    fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    info!(
        "Generating platform catalog from {}",
        platform_dir.display()
    );
    info!("Output directory: {}", output_dir.display());

    // Generate services catalog
    generate_services_catalog(&platform_dir, output_dir)?;

    // Generate deployables catalog
    generate_deployables_catalog(&platform_dir, output_dir)?;

    // Generate resources catalog
    generate_resources_catalog(&platform_dir, output_dir)?;

    // Generate capabilities catalog
    generate_capabilities_catalog(&platform_dir, output_dir)?;

    // Generate topology documentation
    generate_topology_doc(&platform_dir, output_dir)?;

    // Generate architecture overview
    generate_architecture_doc(&platform_dir, output_dir)?;

    info!("✅ Platform catalog generated successfully");
    Ok(())
}

fn load_yaml_models<T: for<'de> Deserialize<'de>>(dir: &Path) -> Result<Vec<T>> {
    let mut models = Vec::new();

    if !dir.exists() {
        return Ok(models);
    }

    let yaml_pattern = format!("{}/*.yaml", dir.display());
    let yaml_files: Vec<PathBuf> = glob::glob(&yaml_pattern)
        .with_context(|| format!("Invalid glob pattern: {}", yaml_pattern))?
        .filter_map(|p| p.ok())
        .collect();

    for yaml_file in &yaml_files {
        let content = fs::read_to_string(yaml_file)
            .with_context(|| format!("Failed to read: {}", yaml_file.display()))?;

        let model: T = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed to parse YAML: {}", yaml_file.display()))?;

        models.push(model);
    }

    Ok(models)
}

fn generate_services_catalog(platform_dir: &Path, output_dir: &Path) -> Result<()> {
    info!("Generating services catalog...");

    let services_dir = platform_dir.join("model/services");
    let services: Vec<ServiceModel> = load_yaml_models(&services_dir)?;

    let catalog_path = output_dir.join("services.generated.yaml");
    let content =
        serde_yaml::to_string(&services).with_context(|| "Failed to serialize services catalog")?;

    fs::write(&catalog_path, content)
        .with_context(|| format!("Failed to write: {}", catalog_path.display()))?;

    info!("  ✓ {} services cataloged", services.len());
    Ok(())
}

fn generate_deployables_catalog(platform_dir: &Path, output_dir: &Path) -> Result<()> {
    info!("Generating deployables catalog...");

    let deployables_dir = platform_dir.join("model/deployables");
    let deployables: Vec<DeployableModel> = load_yaml_models(&deployables_dir)?;

    let catalog_path = output_dir.join("deployables.generated.yaml");
    let content = serde_yaml::to_string(&deployables)
        .with_context(|| "Failed to serialize deployables catalog")?;

    fs::write(&catalog_path, content)
        .with_context(|| format!("Failed to write: {}", catalog_path.display()))?;

    info!("  ✓ {} deployables cataloged", deployables.len());
    Ok(())
}

fn generate_resources_catalog(platform_dir: &Path, output_dir: &Path) -> Result<()> {
    info!("Generating resources catalog...");

    let resources_dir = platform_dir.join("model/resources");
    let resources: Vec<ResourceModel> = load_yaml_models(&resources_dir)?;

    let catalog_path = output_dir.join("resources.generated.yaml");
    let content = serde_yaml::to_string(&resources)
        .with_context(|| "Failed to serialize resources catalog")?;

    fs::write(&catalog_path, content)
        .with_context(|| format!("Failed to write: {}", catalog_path.display()))?;

    info!("  ✓ {} resources cataloged", resources.len());
    Ok(())
}

fn generate_capabilities_catalog(platform_dir: &Path, output_dir: &Path) -> Result<()> {
    info!("Generating capabilities catalog...");

    let capabilities_dir = platform_dir.join("model/capabilities");
    let capabilities: Vec<CapabilityModel> = load_yaml_models(&capabilities_dir)?;

    let catalog_path = output_dir.join("capabilities.generated.yaml");
    let content = serde_yaml::to_string(&capabilities)
        .with_context(|| "Failed to serialize capabilities catalog")?;

    fs::write(&catalog_path, content)
        .with_context(|| format!("Failed to write: {}", catalog_path.display()))?;

    info!("  ✓ {} capabilities cataloged", capabilities.len());
    Ok(())
}

fn generate_topology_doc(platform_dir: &Path, output_dir: &Path) -> Result<()> {
    info!("Generating topology documentation...");

    let topologies_dir = platform_dir.join("model/topologies");
    let topologies: Vec<TopologyModel> = load_yaml_models(&topologies_dir)?;

    let doc_path = output_dir.join("topology.generated.md");
    let mut doc = String::new();

    doc.push_str("# Platform Topologies\n\n");
    doc.push_str("> Generated by platform-generator. DO NOT EDIT.\n\n");

    for topology in &topologies {
        doc.push_str(&format!("## {}\n\n", topology.name));

        if let Some(desc) = &topology.description {
            doc.push_str(&format!("{}\n\n", desc));
        }

        doc.push_str(&format!("- **Version**: {}\n", topology.version));

        // Load full topology YAML for details
        let topo_file = topologies_dir.join(format!("{}.yaml", topology.name));
        if let Ok(content) = fs::read_to_string(&topo_file)
            && let Ok(yaml) = serde_yaml::from_str::<serde_json::Value>(&content)
            && let Some(deployables) = yaml.get("deployables").and_then(|v| v.as_array())
        {
            doc.push_str("\n### Deployables\n\n");
            for dep in deployables {
                let name = dep
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let replicas = dep.get("replicas").and_then(|v| v.as_u64()).unwrap_or(1);
                let enabled = dep.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                let status = if enabled { "✅" } else { "⏸️" };
                doc.push_str(&format!("- {} {} (×{})\n", status, name, replicas));
            }
        }

        doc.push_str("\n---\n\n");
    }

    fs::write(&doc_path, doc)
        .with_context(|| format!("Failed to write: {}", doc_path.display()))?;

    info!("  ✓ {} topologies documented", topologies.len());
    Ok(())
}

fn generate_architecture_doc(platform_dir: &Path, output_dir: &Path) -> Result<()> {
    info!("Generating architecture documentation...");

    let doc_path = output_dir.join("architecture.generated.md");
    let mut doc = String::new();

    doc.push_str("# Platform Architecture Overview\n\n");
    doc.push_str("> Generated by platform-generator. DO NOT EDIT.\n\n");

    // Count models
    let services_dir = platform_dir.join("model/services");
    let deployables_dir = platform_dir.join("model/deployables");
    let resources_dir = platform_dir.join("model/resources");

    let service_count = glob::glob(&format!("{}/*.yaml", services_dir.display()))
        .map(|g| g.count())
        .unwrap_or(0);
    let deployable_count = glob::glob(&format!("{}/*.yaml", deployables_dir.display()))
        .map(|g| g.count())
        .unwrap_or(0);
    let resource_count = glob::glob(&format!("{}/*.yaml", resources_dir.display()))
        .map(|g| g.count())
        .unwrap_or(0);

    doc.push_str(&format!("- **Services**: {}\n", service_count));
    doc.push_str(&format!("- **Deployables**: {}\n", deployable_count));
    doc.push_str(&format!("- **Resources**: {}\n", resource_count));

    doc.push_str("\n## Service Registry\n\n");

    // Collect and sort service entries to ensure deterministic cross-platform output
    let mut services: Vec<(String, String, String, String)> = Vec::new();
    if let Ok(entries) = fs::read_dir(&services_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|e| e.to_str()) == Some("yaml")
                && let Ok(content) = fs::read_to_string(entry.path())
                && let Ok(yaml) = serde_yaml::from_str::<serde_json::Value>(&content)
            {
                let name = yaml
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let domain = yaml
                    .get("domain")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let version = yaml
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.0.0")
                    .to_string();
                let status = yaml
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                services.push((name, domain, version, status));
            }
        }
    }
    // Sort by service name for deterministic output
    services.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, domain, version, status) in &services {
        let status_icon = match status.as_str() {
            "active" => "✅",
            "stub" => "⚠️",
            "pending" => "❌",
            _ => "❓",
        };

        doc.push_str(&format!(
            "- {} **{}** ({}/{})\n",
            status_icon, name, domain, version
        ));
    }

    fs::write(&doc_path, doc)
        .with_context(|| format!("Failed to write: {}", doc_path.display()))?;

    info!("  ✓ Architecture overview generated");
    Ok(())
}
