use std::path::PathBuf;

use anyhow::Result;
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

use crate::commands;
use crate::support::Mode;

#[derive(Parser)]
#[command(name = "repo-tools")]
#[command(about = "Rust repo tooling replacing ts/sh scripts")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Doctor,
    GateGuidance(GateGuidanceArgs),
    RouteTask(RouteTaskArgs),
    ValidateState(ModeArgs),
    ValidateWorkflows(ModeArgs),
    VerifyReplay(ModeArgs),
    BoundaryCheck,
    Typegen,
    DriftCheck,
    SdkDriftCheck,
    Gate(GateArgs),
    VerifyHandoff(VerifyHandoffArgs),
    ValidateExistence(ModeArgs),
    ValidateImports(ModeArgs),
    ValidatePublishIntent(ModeArgs),
    ValidateContractBoundaries(ModeArgs),
    ValidateContracts(ModeArgs),
    GenDirectoryCategories,
    VerifyCounterDelivery(ModeArgs),
    VerifyGeneratedArtifacts,
    CommitGoldenBaseline,
    TemplateInit(TemplateInitArgs),
    AuditBackendCore(AuditBackendCoreArgs),
    SemverCheck(SemverCheckArgs),
    GenerateService(GenerateServiceArgs),
    SetupNats,
    CleanupTestArtifacts,
    CleanSweep,
    CleanSweepDeps,
    RequireTool(RequireToolArgs),
    SetupSccache,
    SetupSccacheVerify,
    SetupHakari,
    SetupHakariVerify,
    SetupCoverage,
    AuditRust,
    ValidateUnsafeCode,
    K6Baseline(K6BaselineArgs),
    ValidateResilience(ModeArgs),
    PlatformServices,
    PlatformDeployables,
    PlatformResources,
    PlatformCapabilities,
    CleanSdk,
    Dev(DevArgs),
    Apps(AppsArgs),
    Secrets(SecretsArgs),
    Infra(InfraArgs),
    Ops(OpsArgs),
    ToolchainCheck,
    ScanVuln(ScanVulnArgs),
    GenSbom(GenSbomArgs),
    SignImage(ImageRefArgs),
    VerifyImage(ImageRefArgs),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum CliMode {
    Warn,
    Strict,
}

impl From<CliMode> for Mode {
    fn from(value: CliMode) -> Self {
        match value {
            CliMode::Warn => Self::Warn,
            CliMode::Strict => Self::Strict,
        }
    }
}

#[derive(Args)]
pub(crate) struct ModeArgs {
    #[arg(long, value_enum, default_value = "warn")]
    mode: CliMode,
}

#[derive(Args)]
pub(crate) struct GateGuidanceArgs {
    #[arg(long, short)]
    pub(crate) list: bool,
    pub(crate) agent: Option<String>,
}

#[derive(Args)]
pub(crate) struct RouteTaskArgs {
    #[arg(long, short)]
    pub(crate) list: bool,
    #[arg(long)]
    pub(crate) paths: Vec<String>,
    #[arg(long)]
    pub(crate) diff: Option<String>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum GateName {
    Local,
    Prepush,
    Ci,
    Release,
}

#[derive(Args)]
pub(crate) struct GateArgs {
    pub(crate) gate: GateName,
    #[arg(long, value_enum)]
    mode: Option<CliMode>,
}

impl GateArgs {
    pub(crate) fn mode(&self) -> Option<Mode> {
        self.mode.map(Into::into)
    }
}

#[derive(Args)]
pub(crate) struct VerifyHandoffArgs {
    pub(crate) agent: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum TemplateProfile {
    BackendCore,
    BackendDesktop,
    FullResearch,
}

impl TemplateProfile {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::BackendCore => "backend-core",
            Self::BackendDesktop => "backend-desktop",
            Self::FullResearch => "full-research",
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum TemplateInitMode {
    DryRun,
    Apply,
}

impl TemplateInitMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::Apply => "apply",
        }
    }
}

#[derive(Args)]
pub(crate) struct TemplateInitArgs {
    #[arg(long, value_enum, default_value = "backend-core")]
    pub(crate) profile: TemplateProfile,
    #[arg(long, value_enum, default_value = "dry-run")]
    pub(crate) mode: TemplateInitMode,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum BackendCoreAuditMode {
    DryRun,
    Strict,
}

impl BackendCoreAuditMode {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::Strict => "strict",
        }
    }
}

#[derive(Args)]
pub(crate) struct AuditBackendCoreArgs {
    #[arg(value_enum, default_value = "dry-run")]
    pub(crate) mode: BackendCoreAuditMode,
}

#[derive(Args)]
pub(crate) struct SemverCheckArgs {
    #[arg(long, default_value = "")]
    pub(crate) baseline: String,
    #[arg(long, default_value = "minor")]
    pub(crate) release_type: String,
    #[arg(long, default_value = "v[0-9]*.[0-9]*.[0-9]*")]
    pub(crate) release_tag_glob: String,
}

#[derive(Args)]
pub(crate) struct GenerateServiceArgs {
    #[arg(long)]
    pub(crate) bin_path: PathBuf,
    #[arg(long)]
    pub(crate) env_file: PathBuf,
    #[arg(long, default_value = "root")]
    pub(crate) user: String,
    #[arg(long, default_value = "root")]
    pub(crate) group: String,
}

#[derive(Args)]
pub(crate) struct RequireToolArgs {
    pub(crate) tool: String,
    pub(crate) install_hint: String,
}

#[derive(Args)]
pub(crate) struct K6BaselineArgs {
    #[arg(long, default_value = "http://localhost:3010")]
    pub(crate) base_url: String,
    #[arg(long, default_value = "30s")]
    pub(crate) duration: String,
    #[arg(long, default_value = "5")]
    pub(crate) vus: String,
}

#[derive(Args)]
pub(crate) struct DevArgs {
    #[command(subcommand)]
    pub(crate) command: DevCommand,
}

#[derive(Subcommand)]
pub(crate) enum DevCommand {
    AgeKey(DevAgeKeyArgs),
    Process(DevProcessArgs),
    Migration(DevMigrationArgs),
    Worker(DevWorkerArgs),
    Skills(DevSkillsArgs),
}

#[derive(Args)]
pub(crate) struct DevAgeKeyArgs {
    #[command(subcommand)]
    pub(crate) command: DevAgeKeyCommand,
}

#[derive(Subcommand)]
pub(crate) enum DevAgeKeyCommand {
    Generate(DevAgeKeyGenerateArgs),
    Show,
}

#[derive(Args)]
pub(crate) struct DevAgeKeyGenerateArgs {
    #[arg(long)]
    pub(crate) overwrite: bool,
}

#[derive(Args)]
pub(crate) struct DevProcessArgs {
    #[command(subcommand)]
    pub(crate) command: DevProcessCommand,
}

#[derive(Subcommand)]
pub(crate) enum DevProcessCommand {
    Status,
    Ports,
    Stop,
    StopPort(DevStopPortArgs),
    CleanOrphans,
}

#[derive(Args)]
pub(crate) struct DevStopPortArgs {
    pub(crate) port: u16,
}

#[derive(Args)]
pub(crate) struct DevMigrationArgs {
    #[command(subcommand)]
    pub(crate) command: DevMigrationCommand,
}

#[derive(Subcommand)]
pub(crate) enum DevMigrationCommand {
    Create(DevCreateMigrationArgs),
}

#[derive(Args, Clone)]
pub(crate) struct DevCreateMigrationArgs {
    pub(crate) name: String,
}

#[derive(Args)]
pub(crate) struct DevWorkerArgs {
    #[command(subcommand)]
    pub(crate) command: DevWorkerCommand,
}

#[derive(Subcommand)]
pub(crate) enum DevWorkerCommand {
    Start(DevWorkerStartArgs),
    Stop,
    Status,
    Health,
    Run(DevWorkerRunArgs),
}

#[derive(Args)]
pub(crate) struct DevWorkerStartArgs {
    #[arg(long)]
    pub(crate) attach: bool,
}

#[derive(Args)]
pub(crate) struct DevWorkerRunArgs {
    pub(crate) worker: String,
}

#[derive(Args)]
pub(crate) struct DevSkillsArgs {
    #[command(subcommand)]
    pub(crate) command: DevSkillsCommand,
}

#[derive(Subcommand)]
pub(crate) enum DevSkillsCommand {
    List,
    Find(DevSkillsFindArgs),
    Check,
    Add(DevSkillsAddArgs),
    AddSpecific(DevSkillsAddSpecificArgs),
    Update,
    Remove(DevSkillsRemoveArgs),
    Init(DevSkillsInitArgs),
    Status,
}

#[derive(Args)]
pub(crate) struct DevSkillsFindArgs {
    pub(crate) query: Option<String>,
}

#[derive(Args)]
pub(crate) struct DevSkillsAddArgs {
    pub(crate) source: String,
}

#[derive(Args)]
pub(crate) struct DevSkillsAddSpecificArgs {
    pub(crate) source: String,
    pub(crate) skill: String,
}

#[derive(Args)]
pub(crate) struct DevSkillsRemoveArgs {
    pub(crate) skill: String,
}

#[derive(Args)]
pub(crate) struct DevSkillsInitArgs {
    pub(crate) name: String,
}

#[derive(Args)]
pub(crate) struct SecretsArgs {
    #[command(subcommand)]
    pub(crate) command: SecretsCommand,
}

#[derive(Subcommand)]
pub(crate) enum SecretsCommand {
    DecryptEnv(SecretsDecryptEnvArgs),
    ExportEnv(SecretsExportEnvArgs),
    VerifyCounterDbCredentials(SecretsEnvArgs),
    Run(SecretsRunArgs),
    Reconcile(SecretsReconcileArgs),
    Encrypt(SecretsEncryptArgs),
    Edit(SecretsEditArgs),
    SetupFluxSecret,
    Validate,
}

#[derive(Args)]
pub(crate) struct SecretsDecryptEnvArgs {
    pub(crate) file: PathBuf,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum SecretsExportProfile {
    #[value(name = "systemd-binary")]
    SystemdBinary,
    Podman,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum DatabaseCapabilityState {
    Disabled,
    #[value(name = "local_mock")]
    LocalMock,
    #[value(name = "local_real")]
    LocalReal,
    #[value(name = "external_single_node")]
    ExternalSingleNode,
    #[value(name = "external_distributed")]
    ExternalDistributed,
}

impl DatabaseCapabilityState {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::LocalMock => "local_mock",
            Self::LocalReal => "local_real",
            Self::ExternalSingleNode => "external_single_node",
            Self::ExternalDistributed => "external_distributed",
        }
    }

    pub(crate) fn include_counter_db_credentials(self) -> bool {
        matches!(self, Self::ExternalSingleNode | Self::ExternalDistributed)
    }
}

impl SecretsExportProfile {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::SystemdBinary => "systemd-binary",
            Self::Podman => "podman",
        }
    }
}

#[derive(Args)]
pub(crate) struct SecretsExportEnvArgs {
    #[arg(long, default_value = "web-bff")]
    pub(crate) deployable: String,
    #[arg(long, default_value = "dev")]
    pub(crate) env: String,
    #[arg(long, value_enum, default_value = "systemd-binary")]
    pub(crate) profile: SecretsExportProfile,
    #[arg(long)]
    pub(crate) output: PathBuf,
}

#[derive(Args)]
pub(crate) struct SecretsEnvArgs {
    #[arg(long, default_value = "dev")]
    pub(crate) env: String,
}

#[derive(Args)]
pub(crate) struct SecretsRunArgs {
    #[arg(long, default_value = "web-bff")]
    pub(crate) deployable: String,
    #[arg(long, default_value = "dev")]
    pub(crate) env: String,
    #[arg(long, value_enum, default_value = "local_real")]
    pub(crate) db_state: DatabaseCapabilityState,
    #[arg(last = true)]
    pub(crate) cmd: Vec<String>,
}

#[derive(Args)]
pub(crate) struct SecretsReconcileArgs {
    #[arg(long, default_value = "dev")]
    pub(crate) env: String,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Args)]
pub(crate) struct SecretsEncryptArgs {
    #[arg(long, default_value = "web-bff")]
    pub(crate) deployable: String,
    #[arg(long, default_value = "dev")]
    pub(crate) env: String,
}

#[derive(Args)]
pub(crate) struct SecretsEditArgs {
    #[arg(long, default_value = "web-bff")]
    pub(crate) deployable: String,
    #[arg(long, default_value = "dev")]
    pub(crate) env: String,
}

#[derive(Args)]
pub(crate) struct InfraArgs {
    #[command(subcommand)]
    pub(crate) command: InfraCommand,
}

#[derive(Subcommand)]
pub(crate) enum InfraCommand {
    Auth(InfraAuthArgs),
    Local(InfraLocalArgs),
    K3s(InfraK3sArgs),
}

#[derive(Args)]
pub(crate) struct InfraLocalArgs {
    #[command(subcommand)]
    pub(crate) command: InfraLocalCommand,
}

#[derive(Subcommand)]
pub(crate) enum InfraLocalCommand {
    Up(InfraLocalUpArgs),
    Down(InfraLocalDownArgs),
    Status(InfraLocalStatusArgs),
    Logs(InfraLocalLogsArgs),
    Observability(InfraObservabilityArgs),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum ContainerRuntime {
    Docker,
    Podman,
}

impl ContainerRuntime {
    pub(crate) fn binary(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Docker => "docker compose",
            Self::Podman => "podman compose",
        }
    }
}

#[derive(Args)]
pub(crate) struct InfraLocalUpArgs {
    #[arg(long, value_enum)]
    pub(crate) runtime: Option<ContainerRuntime>,
    #[arg(long)]
    pub(crate) profile: Vec<String>,
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    pub(crate) detach: bool,
}

#[derive(Args)]
pub(crate) struct InfraLocalDownArgs {
    #[arg(long, value_enum)]
    pub(crate) runtime: Option<ContainerRuntime>,
    #[arg(long)]
    pub(crate) volumes: bool,
}

#[derive(Args)]
pub(crate) struct InfraLocalStatusArgs {
    #[arg(long, value_enum)]
    pub(crate) runtime: Option<ContainerRuntime>,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct InfraLocalLogsArgs {
    #[arg(long, value_enum)]
    pub(crate) runtime: Option<ContainerRuntime>,
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, short)]
    pub(crate) follow: bool,
}

#[derive(Args)]
pub(crate) struct InfraObservabilityArgs {
    #[command(subcommand)]
    pub(crate) command: InfraObservabilityCommand,
}

#[derive(Subcommand)]
pub(crate) enum InfraObservabilityCommand {
    Up(InfraObservabilityUpArgs),
    Down(InfraObservabilityDownArgs),
    Status(InfraObservabilityStatusArgs),
    Logs(InfraObservabilityLogsArgs),
}

#[derive(Args)]
pub(crate) struct InfraObservabilityUpArgs {
    #[arg(long, value_enum)]
    pub(crate) runtime: Option<ContainerRuntime>,
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    pub(crate) detach: bool,
}

#[derive(Args)]
pub(crate) struct InfraObservabilityDownArgs {
    #[arg(long, value_enum)]
    pub(crate) runtime: Option<ContainerRuntime>,
}

#[derive(Args)]
pub(crate) struct InfraObservabilityStatusArgs {
    #[arg(long, value_enum)]
    pub(crate) runtime: Option<ContainerRuntime>,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct InfraObservabilityLogsArgs {
    #[arg(long, value_enum)]
    pub(crate) runtime: Option<ContainerRuntime>,
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, short)]
    pub(crate) follow: bool,
}

#[derive(Args)]
pub(crate) struct InfraAuthArgs {
    #[command(subcommand)]
    pub(crate) command: InfraAuthCommand,
}

#[derive(Subcommand)]
pub(crate) enum InfraAuthCommand {
    Up,
    Bootstrap,
    Down,
    Status,
    Logs,
}

#[derive(Args)]
pub(crate) struct InfraK3sArgs {
    #[command(subcommand)]
    pub(crate) command: InfraK3sCommand,
}

#[derive(Subcommand)]
pub(crate) enum InfraK3sCommand {
    Deploy(InfraK3sDeployArgs),
    Bootstrap(InfraK3sBootstrapArgs),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum KubectlDryRunMode {
    Client,
    Server,
}

impl KubectlDryRunMode {
    pub(crate) fn arg(self) -> &'static str {
        match self {
            Self::Client => "--dry-run=client",
            Self::Server => "--dry-run=server",
        }
    }
}

#[derive(Args)]
pub(crate) struct InfraK3sDeployArgs {
    #[arg(long, default_value = "dev")]
    pub(crate) env: String,
    #[arg(long, value_enum)]
    pub(crate) dry_run: Option<KubectlDryRunMode>,
    #[arg(long)]
    pub(crate) skip_verify: bool,
}

#[derive(Args)]
pub(crate) struct InfraK3sBootstrapArgs {
    #[arg(long)]
    pub(crate) apply: bool,
    #[arg(long)]
    pub(crate) i_understand_this_modifies_host: bool,
}

#[derive(Args)]
pub(crate) struct OpsArgs {
    #[command(subcommand)]
    pub(crate) command: OpsCommand,
}

#[derive(Subcommand)]
pub(crate) enum OpsCommand {
    Migrate(OpsMigrateArgs),
    BootstrapVps(OpsBootstrapVpsArgs),
    Service(OpsServiceArgs),
}

#[derive(Args)]
pub(crate) struct OpsServiceArgs {
    #[command(subcommand)]
    pub(crate) command: OpsServiceCommand,
}

#[derive(Subcommand)]
pub(crate) enum OpsServiceCommand {
    Deploy(OpsServiceNameArgs),
    Stop(OpsServiceNameArgs),
    Logs(OpsServiceLogsArgs),
}

#[derive(Args)]
pub(crate) struct OpsServiceNameArgs {
    #[arg(long, default_value = "axum-api")]
    pub(crate) service: String,
}

#[derive(Args)]
pub(crate) struct OpsServiceLogsArgs {
    #[arg(long, default_value = "axum-api")]
    pub(crate) service: String,
    #[arg(long, short, default_value_t = true, action = ArgAction::Set)]
    pub(crate) follow: bool,
}

#[derive(Args)]
pub(crate) struct ScanVulnArgs {
    #[arg(long, default_value = "")]
    pub(crate) image: String,
    #[arg(long, default_value_t = 0)]
    pub(crate) exit_code: i32,
}

#[derive(Args)]
pub(crate) struct GenSbomArgs {
    #[arg(long, default_value = "")]
    pub(crate) image: String,
    #[arg(long, default_value = "json")]
    pub(crate) format: String,
    #[arg(long, default_value = "-")]
    pub(crate) output: String,
}

#[derive(Args)]
pub(crate) struct ImageRefArgs {
    pub(crate) image: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum OpsMigrationDirection {
    Up,
    Down,
    Status,
    Reset,
}

#[derive(Args)]
pub(crate) struct OpsMigrateArgs {
    #[arg(long, default_value = "local")]
    pub(crate) env: String,
    #[arg(long, value_enum, default_value = "up")]
    pub(crate) direction: OpsMigrationDirection,
    #[arg(long)]
    pub(crate) dry_run: bool,
    #[arg(long)]
    pub(crate) apply: bool,
}

#[derive(Args)]
pub(crate) struct OpsBootstrapVpsArgs {
    #[arg(long)]
    pub(crate) plan: bool,
    #[arg(long)]
    pub(crate) apply: bool,
    #[arg(long)]
    pub(crate) confirm: Option<String>,
}

#[derive(Args)]
pub(crate) struct AppsArgs {
    #[command(subcommand)]
    pub(crate) command: AppsCommand,
}

#[derive(Subcommand)]
pub(crate) enum AppsCommand {
    E2e(AppsE2eArgs),
    DevDesktop(AppsDevDesktopArgs),
    BoundaryAudit(AppsBoundaryAuditArgs),
}

#[derive(Args)]
pub(crate) struct AppsBoundaryAuditArgs {
    #[arg(value_enum, default_value = "dry-run")]
    pub(crate) mode: BackendCoreAuditMode,
}

#[derive(Args)]
pub(crate) struct AppsE2eArgs {
    #[command(subcommand)]
    pub(crate) command: AppsE2eCommand,
}

#[derive(Subcommand)]
pub(crate) enum AppsE2eCommand {
    Preflight,
    Run,
}

#[derive(Args)]
pub(crate) struct AppsDevDesktopArgs {
    #[arg(long)]
    pub(crate) dry_run: bool,
}

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Doctor => commands::doctor::doctor(),
        Commands::GateGuidance(args) => commands::harness::gate_guidance(args),
        Commands::RouteTask(args) => commands::harness::route_task(args),
        Commands::ValidateState(args) => commands::platform::validate_state(args.mode.into()),
        Commands::ValidateWorkflows(args) => {
            commands::platform::validate_workflows(args.mode.into())
        }
        Commands::VerifyReplay(args) => commands::workers::verify_replay(args.mode.into()),
        Commands::BoundaryCheck => commands::contracts::boundary_check(),
        Commands::Typegen => commands::contracts::typegen(),
        Commands::DriftCheck => commands::contracts::drift_check(),
        Commands::SdkDriftCheck => commands::contracts::sdk_drift_check(),
        Commands::Gate(args) => commands::harness::gate(args),
        Commands::VerifyHandoff(args) => commands::harness::verify_handoff(args),
        Commands::ValidateExistence(args) => {
            commands::harness::validate_existence(args.mode.into())
        }
        Commands::ValidateImports(args) => commands::harness::validate_imports(args.mode.into()),
        Commands::ValidatePublishIntent(args) => {
            commands::harness::validate_publish_intent(args.mode.into())
        }
        Commands::ValidateContractBoundaries(args) => {
            commands::contracts::validate_contract_boundaries(args.mode.into())
        }
        Commands::ValidateContracts(args) => {
            commands::contracts::validate_contracts(args.mode.into())
        }
        Commands::GenDirectoryCategories => commands::platform::gen_directory_categories(),
        Commands::VerifyCounterDelivery(args) => {
            commands::platform::verify_counter_delivery(args.mode.into())
        }
        Commands::VerifyGeneratedArtifacts => commands::platform::verify_generated_artifacts(),
        Commands::CommitGoldenBaseline => commands::platform::commit_golden_baseline(),
        Commands::TemplateInit(args) => commands::template::template_init(args),
        Commands::AuditBackendCore(args) => commands::template::audit_backend_core(args),
        Commands::SemverCheck(args) => commands::template::semver_check(args),
        Commands::GenerateService(args) => commands::devx::generate_service(args),
        Commands::SetupNats => commands::devx::setup_nats(),
        Commands::CleanupTestArtifacts => commands::devx::cleanup_test_artifacts(),
        Commands::CleanSweep => commands::devx::clean_sweep(),
        Commands::CleanSweepDeps => commands::devx::clean_sweep_deps(),
        Commands::RequireTool(args) => commands::devx::require_tool_cmd(args),
        Commands::SetupSccache => commands::devx::setup_sccache(),
        Commands::SetupSccacheVerify => commands::devx::setup_sccache_verify(),
        Commands::SetupHakari => commands::devx::setup_hakari(),
        Commands::SetupHakariVerify => commands::devx::setup_hakari_verify(),
        Commands::SetupCoverage => commands::devx::setup_coverage(),
        Commands::AuditRust => commands::devx::audit_rust(),
        Commands::ValidateUnsafeCode => commands::devx::validate_unsafe_code(),
        Commands::K6Baseline(args) => commands::devx::k6_baseline(args),
        Commands::ValidateResilience(args) => {
            commands::workers::validate_resilience(args.mode.into())
        }
        Commands::PlatformServices => commands::platform::list_platform_inventory("services"),
        Commands::PlatformDeployables => commands::platform::list_platform_inventory("deployables"),
        Commands::PlatformResources => commands::platform::list_platform_inventory("resources"),
        Commands::PlatformCapabilities => {
            commands::platform::list_platform_inventory("capabilities")
        }
        Commands::CleanSdk => commands::platform::clean_sdk(),
        Commands::Dev(args) => commands::devx::run_dev(args),
        Commands::Apps(args) => commands::apps::run(args),
        Commands::Secrets(args) => commands::secrets::run(args),
        Commands::Infra(args) => commands::infra::run(args),
        Commands::Ops(args) => commands::ops::run(args),
        Commands::ToolchainCheck => commands::doctor::toolchain_check(),
        Commands::ScanVuln(args) => commands::devx::scan_vuln(args),
        Commands::GenSbom(args) => commands::devx::gen_sbom(args),
        Commands::SignImage(args) => commands::devx::sign_image(args),
        Commands::VerifyImage(args) => commands::devx::verify_image(args),
    }
}
