use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::cli::{
    SecretsArgs, SecretsCommand, SecretsDecryptEnvArgs, SecretsEditArgs, SecretsEncryptArgs,
    SecretsEnvArgs, SecretsExportEnvArgs, SecretsReconcileArgs, SecretsRunArgs,
};
use crate::support::{
    collect_files_with_extension, normalize_slashes, read, require_tool, user_home_dir,
    workspace_root,
};

const AGE_KEY_RELATIVE_PATH: &str = ".config/sops/age/key.txt";
const SHARED_COUNTER_DB: &str = "counter-shared-db";

pub(crate) fn run(args: SecretsArgs) -> Result<()> {
    match args.command {
        SecretsCommand::DecryptEnv(args) => decrypt_env(args),
        SecretsCommand::ExportEnv(args) => export_env(args),
        SecretsCommand::VerifyCounterSharedDb(args) => verify_counter_shared_db_cli(args),
        SecretsCommand::Run(args) => run_with_secrets(args),
        SecretsCommand::Reconcile(args) => reconcile(args),
        SecretsCommand::Encrypt(args) => encrypt(args),
        SecretsCommand::Edit(args) => edit(args),
        SecretsCommand::SetupFluxSecret => setup_flux_secret(),
        SecretsCommand::Validate => validate(),
    }
}

pub(crate) fn verify_counter_shared_db(env: &str) -> Result<()> {
    let root = workspace_root()?;
    let secret = counter_shared_db_path(&root, env);
    if !secret.is_file() {
        bail!("counter-shared-db secret not found: {}", secret.display());
    }

    let exports = decrypt_exports(&secret)?;
    let app_url = require_export(&exports, "APP_TURSO_URL")?;
    let app_token = require_export(&exports, "APP_TURSO_AUTH_TOKEN")?;
    let outbox_url = require_export(&exports, "OUTBOX_DATABASE_URL")?;
    let outbox_token = require_export(&exports, "OUTBOX_TURSO_AUTH_TOKEN")?;
    let projector_url = require_export(&exports, "PROJECTOR_DATABASE_URL")?;
    let projector_token = require_export(&exports, "PROJECTOR_TURSO_AUTH_TOKEN")?;

    require_libsql_url("APP_TURSO_URL", app_url)?;
    require_libsql_url("OUTBOX_DATABASE_URL", outbox_url)?;
    require_libsql_url("PROJECTOR_DATABASE_URL", projector_url)?;
    require_real_token("APP_TURSO_AUTH_TOKEN", app_token)?;
    require_real_token("OUTBOX_TURSO_AUTH_TOKEN", outbox_token)?;
    require_real_token("PROJECTOR_TURSO_AUTH_TOKEN", projector_token)?;

    if app_url != outbox_url || app_url != projector_url {
        bail!("shared counter DB URLs are not aligned across web-bff/outbox/projector");
    }

    println!("counter-shared-db secret verified");
    println!("  url: {app_url}");
    println!("  app token: {}", mask_token(app_token));
    println!("  outbox token: {}", mask_token(outbox_token));
    println!("  projector token: {}", mask_token(projector_token));
    Ok(())
}

fn decrypt_env(args: SecretsDecryptEnvArgs) -> Result<()> {
    let exports = decrypt_exports(&args.file)?;
    println!("Decrypted environment keys from {}:", args.file.display());
    for key in exports.keys() {
        println!("export {key}=<redacted>");
    }
    Ok(())
}

fn export_env(args: SecretsExportEnvArgs) -> Result<()> {
    let root = workspace_root()?;
    require_tool("sops", "mise install")?;
    let exports = collect_deployable_exports(&root, &args.env, &args.deployable)?;
    if exports.is_empty() {
        bail!(
            "no encrypted secrets found for {} in {}; refusing to write empty env file",
            args.deployable,
            args.env
        );
    }

    write_env_file(&args.output, &exports)?;
    println!("Exported SOPS environment file");
    println!("  profile: {}", args.profile.label());
    println!("  environment: {}", args.env);
    println!("  deployable: {}", args.deployable);
    println!("  output: {}", args.output.display());
    println!("  keys: {}", exports.len());
    println!(
        "Use this file as a transient host secret for systemd EnvironmentFile or podman --env-file."
    );
    Ok(())
}

fn verify_counter_shared_db_cli(args: SecretsEnvArgs) -> Result<()> {
    verify_counter_shared_db(&args.env)
}

fn run_with_secrets(args: SecretsRunArgs) -> Result<()> {
    let root = workspace_root()?;
    require_tool("sops", "mise install")?;
    let env_vars = collect_deployable_exports(&root, &args.env, &args.deployable)?;

    if env_vars.is_empty() {
        eprintln!(
            "WARNING: no encrypted secrets found for {} in {}; running with default environment",
            args.deployable, args.env
        );
    }

    let command = if args.cmd.is_empty() {
        vec![
            OsString::from("cargo"),
            OsString::from("run"),
            OsString::from("-p"),
            OsString::from(&args.deployable),
        ]
    } else {
        args.cmd.iter().map(OsString::from).collect::<Vec<_>>()
    };
    let (program, command_args) = command
        .split_first()
        .context("missing command to run with secrets")?;

    println!("Running: {}", display_command(&command));
    println!("Environment: {}", args.env);
    println!("Deployable: {}", args.deployable);
    println!("Injected secret keys: {}", env_vars.len());

    let status = Command::new(program)
        .args(command_args)
        .current_dir(root)
        .envs(env_vars)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to run {}", program.to_string_lossy()))?;
    if !status.success() {
        bail!(
            "secret command failed with status {}",
            status.code().unwrap_or(1)
        );
    }
    Ok(())
}

fn collect_deployable_exports(
    root: &Path,
    env: &str,
    deployable: &str,
) -> Result<BTreeMap<String, String>> {
    let mut env_vars = BTreeMap::new();
    let deployable_path = secret_path(root, env, deployable);
    if deployable_path.is_file() {
        env_vars.extend(decrypt_exports(&deployable_path)?);
    } else if deployable == SHARED_COUNTER_DB {
        let shared_path = counter_shared_db_path(root, env);
        if shared_path.is_file() {
            env_vars.extend(decrypt_exports(&shared_path)?);
        }
    } else {
        bail!(
            "deployable secret not found for {} in {}: {}",
            deployable,
            env,
            deployable_path.display()
        );
    }

    let shared_path = counter_shared_db_path(root, env);
    if shared_path.is_file() && deployable != SHARED_COUNTER_DB {
        env_vars.extend(decrypt_exports(&shared_path)?);
    }

    Ok(env_vars)
}

fn write_env_file(path: &Path, exports: &BTreeMap<String, String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut content = String::new();
    content.push_str("# Generated by repo-tools secrets export-env. Do not commit.\n");
    for (key, value) in exports {
        content.push_str(key);
        content.push('=');
        content.push_str(&quote_env_value(value));
        content.push('\n');
    }

    let mut options = OpenOptions::new();
    options.create(true).write(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options
        .open(path)
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = file
            .metadata()
            .with_context(|| format!("failed to stat {}", path.display()))?
            .permissions();
        permissions.set_mode(0o600);
        file.set_permissions(permissions)
            .with_context(|| format!("failed to chmod 0600 {}", path.display()))?;
    }

    Ok(())
}

fn quote_env_value(value: &str) -> String {
    if value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':' | b'@' | b'%')
    }) {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\\', "\\\\").replace('\'', "'\\''"))
    }
}

fn reconcile(args: SecretsReconcileArgs) -> Result<()> {
    let root = workspace_root()?;
    require_tool("sops", "mise install")?;
    if !args.dry_run {
        require_tool("kubectl", "install kubectl and configure cluster access")?;
    }

    let sops_dir = root.join("infra/security/sops").join(&args.env);
    if !sops_dir.is_dir() {
        bail!("SOPS directory not found: {}", sops_dir.display());
    }

    let mut secret_files = collect_files_with_extension(&sops_dir, "yaml")
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".enc.yaml"))
        })
        .collect::<Vec<_>>();
    secret_files.sort();

    println!(
        "Applying SOPS-encrypted secrets for environment: {}",
        args.env
    );
    println!("SOPS directory: {}", sops_dir.display());
    if secret_files.is_empty() {
        println!("No encrypted secrets found for environment: {}", args.env);
        return Ok(());
    }

    for secret_file in secret_files {
        let secret_name = secret_file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .trim_end_matches(".enc.yaml");
        println!("Decrypting and applying: {secret_name}");
        let plain = decrypt_plain(&secret_file)?;
        if args.dry_run {
            let value = parse_yaml(&plain)?;
            let kind = value
                .get("kind")
                .and_then(serde_yaml::Value::as_str)
                .unwrap_or("unknown");
            let name = value
                .get("metadata")
                .and_then(|metadata| metadata.get("name"))
                .and_then(serde_yaml::Value::as_str)
                .unwrap_or(secret_name);
            println!("DRY-RUN: would apply {kind}/{name}");
        } else {
            kubectl_apply(&plain)?;
            println!("✓ Applied: {secret_name}");
        }
    }

    println!("All secrets processed for environment: {}", args.env);
    Ok(())
}

fn validate() -> Result<()> {
    let root = workspace_root()?;
    let sops_config = root.join(".sops.yaml");
    if !sops_config.is_file() {
        bail!(".sops.yaml not found at repo root");
    }
    println!("✓ .sops.yaml exists");

    let age_key = age_key_path()?;
    if !age_key.is_file() {
        bail!("age key not found. Generate with: just sops-gen-age-key");
    }
    println!("✓ Age key exists");

    let public_key = read_age_public_key(&age_key)?;
    let sops_yaml = read(&sops_config)?;
    if !sops_yaml.contains(&public_key) {
        bail!(
            "age public key is not present in .sops.yaml; run just sops-show-age-key and update creation_rules"
        );
    }
    println!("✓ Age public key matches .sops.yaml");

    require_tool("sops", "mise install")?;
    println!("✓ sops available");

    let encrypted = collect_files_with_extension(root.join("infra/security/sops"), "yaml")
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".enc.yaml"))
        })
        .collect::<Vec<_>>();
    println!("✓ Encrypted secrets: {} files", encrypted.len());

    if let Some(sample) = encrypted
        .iter()
        .filter(|path| normalize_slashes(path).contains("infra/security/sops/dev/"))
        .min()
    {
        decrypt_exports(sample).with_context(|| {
            format!(
                "sample decrypt failed: {}; check that SOPS_AGE_KEY_FILE matches .sops.yaml recipients",
                sample.display()
            )
        })?;
        println!("✓ Sample decrypt succeeded: {}", sample.display());
    } else {
        println!("WARNING: no dev encrypted secrets found under infra/security/sops/dev");
    }

    println!("SOPS configuration valid");
    Ok(())
}

fn encrypt(args: SecretsEncryptArgs) -> Result<()> {
    require_tool("sops", "mise install")?;
    let root = workspace_root()?;
    let source = root
        .join("infra/security/sops/templates")
        .join(&args.env)
        .join(format!("{}.yaml", args.deployable));
    if !source.is_file() {
        bail!("template secret not found: {}", source.display());
    }
    let target = secret_path(&root, &args.env, &args.deployable);
    println!(
        "Encrypting secrets for deployable: {} ({})",
        args.deployable, args.env
    );
    let output = Command::new("sops")
        .args(["--encrypt", "--input-type", "yaml", "--output-type", "yaml"])
        .arg(&source)
        .output()
        .with_context(|| format!("failed to encrypt {}", source.display()))?;
    if !output.status.success() {
        bail!("sops encrypt failed for {}", source.display());
    }
    std::fs::write(&target, output.stdout)
        .with_context(|| format!("failed to write {}", target.display()))?;
    println!("Encrypted: {}", target.display());
    Ok(())
}

fn edit(args: SecretsEditArgs) -> Result<()> {
    require_tool("sops", "mise install")?;
    let root = workspace_root()?;
    let target = secret_path(&root, &args.env, &args.deployable);
    if !target.is_file() {
        bail!("encrypted secret not found: {}", target.display());
    }
    println!("Editing: {} ({})", args.deployable, args.env);
    let status = Command::new("sops")
        .arg(&target)
        .status()
        .with_context(|| format!("failed to open sops for {}", target.display()))?;
    if !status.success() {
        bail!(
            "sops edit failed with status {}",
            status.code().unwrap_or(1)
        );
    }
    println!("Saved (encrypted in place)");
    Ok(())
}

fn setup_flux_secret() -> Result<()> {
    require_tool("kubectl", "install kubectl and configure cluster access")?;
    let age_key = age_key_path()?;
    if !age_key.is_file() {
        bail!(
            "age key not found at {}. Generate one with `cargo run -p repo-tools -- dev age-key generate`",
            age_key.display()
        );
    }

    println!("Creating Flux SOPS secret in flux-system namespace");
    let output = Command::new("kubectl")
        .args([
            "create",
            "secret",
            "generic",
            "sops-age",
            "--namespace",
            "flux-system",
            "--from-file",
        ])
        .arg(format!("age.agekey={}", age_key.display()))
        .args(["--dry-run=client", "-o", "yaml"])
        .output()
        .context("failed to generate flux secret manifest")?;
    if !output.status.success() {
        bail!("kubectl create secret failed");
    }

    kubectl_apply(&String::from_utf8_lossy(&output.stdout))?;
    println!("Flux SOPS secret created");
    println!("Flux will now be able to decrypt SOPS-encrypted secrets");
    Ok(())
}

fn decrypt_exports(path: &Path) -> Result<BTreeMap<String, String>> {
    let plain = decrypt_plain(path)?;
    parse_env_exports(&plain)
}

fn decrypt_plain(path: &Path) -> Result<String> {
    require_tool("sops", "mise install")?;
    let mut command = Command::new("sops");
    command.arg("--decrypt").arg(path);
    command.env("SOPS_AGE_KEY_FILE", sops_age_key_file()?);
    let output = command
        .output()
        .with_context(|| format!("failed to decrypt {}", path.display()))?;
    if !output.status.success() {
        bail!("failed to decrypt {}", path.display());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn parse_env_exports(plain: &str) -> Result<BTreeMap<String, String>> {
    let value = parse_yaml(plain)?;
    let mapping = value
        .get("stringData")
        .and_then(serde_yaml::Value::as_mapping)
        .or_else(|| value.as_mapping())
        .context("decrypted YAML does not contain stringData or flat key-value mapping")?;
    let mut exports = BTreeMap::new();
    for (key, value) in mapping {
        if let (Some(key), Some(value)) = (key.as_str(), value.as_str()) {
            exports.insert(key.to_string(), value.to_string());
        }
    }
    Ok(exports)
}

fn parse_yaml(plain: &str) -> Result<serde_yaml::Value> {
    serde_yaml::from_str(plain).context("failed to parse decrypted YAML")
}

fn kubectl_apply(plain: &str) -> Result<()> {
    let mut child = Command::new("kubectl")
        .args(["apply", "-f", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("failed to start kubectl apply")?;
    child
        .stdin
        .as_mut()
        .context("failed to open kubectl stdin")?
        .write_all(plain.as_bytes())
        .context("failed to pass decrypted secret to kubectl")?;
    let status = child.wait().context("failed to wait for kubectl apply")?;
    if !status.success() {
        bail!(
            "kubectl apply failed with status {}",
            status.code().unwrap_or(1)
        );
    }
    Ok(())
}

fn require_export<'a>(exports: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
    exports
        .get(key)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{key} is empty or missing"))
}

fn require_libsql_url(name: &str, value: &str) -> Result<()> {
    if value.starts_with("file:") {
        bail!("{name} still points to local file path: {value}");
    }
    if !value.starts_with("libsql://") {
        bail!("{name} must use libsql:// URL, got: {value}");
    }
    Ok(())
}

fn require_real_token(name: &str, value: &str) -> Result<()> {
    if value == "REPLACE_WITH_TURSO_TOKEN" {
        bail!("{name} still uses template placeholder");
    }
    Ok(())
}

fn mask_token(value: &str) -> String {
    if value.len() <= 8 {
        "***".to_string()
    } else {
        format!("{}***{}", &value[..4], &value[value.len() - 4..])
    }
}

fn secret_path(root: &Path, env: &str, deployable: &str) -> PathBuf {
    root.join("infra/security/sops")
        .join(env)
        .join(format!("{deployable}.enc.yaml"))
}

fn counter_shared_db_path(root: &Path, env: &str) -> PathBuf {
    secret_path(root, env, SHARED_COUNTER_DB)
}

fn sops_age_key_file() -> Result<OsString> {
    if let Some(value) = std::env::var_os("SOPS_AGE_KEY_FILE") {
        Ok(value)
    } else {
        Ok(age_key_path()?.into_os_string())
    }
}

fn age_key_path() -> Result<PathBuf> {
    Ok(user_home_dir()?.join(AGE_KEY_RELATIVE_PATH))
}

fn read_age_public_key(path: &Path) -> Result<String> {
    for line in read(path)?.lines() {
        if let Some(key) = line.strip_prefix("# public key: ") {
            return Ok(key.to_string());
        }
    }
    bail!("could not read public key from {}", path.display())
}

fn display_command(command: &[OsString]) -> String {
    command
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kubernetes_secret_string_data() {
        let exports = parse_env_exports("stringData:\n  A: one\n  B: two\n").unwrap();
        assert_eq!(exports.get("A").map(String::as_str), Some("one"));
        assert_eq!(exports.get("B").map(String::as_str), Some("two"));
    }

    #[test]
    fn parses_flat_string_mapping() {
        let exports = parse_env_exports("A: one\nB: two\n").unwrap();
        assert_eq!(exports.get("A").map(String::as_str), Some("one"));
        assert_eq!(exports.get("B").map(String::as_str), Some("two"));
    }

    #[test]
    fn masks_short_and_long_tokens() {
        assert_eq!(mask_token("short"), "***");
        assert_eq!(mask_token("abcd1234wxyz"), "abcd***wxyz");
    }

    #[test]
    fn quotes_env_values_for_shell_env_files() {
        assert_eq!(quote_env_value("abc-123_:/@%"), "abc-123_:/@%");
        assert_eq!(quote_env_value("hello world"), "'hello world'");
        assert_eq!(quote_env_value("it's ok"), "'it'\\''s ok'");
    }
}
