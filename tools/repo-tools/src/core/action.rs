#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ActionSpec {
    pub(crate) id: &'static str,
    pub(crate) canonical_cli: &'static [&'static str],
    pub(crate) legacy_cli_aliases: &'static [&'static [&'static str]],
    pub(crate) just_recipe: Option<&'static str>,
    pub(crate) class: ActionClass,
    pub(crate) domain: Domain,
    pub(crate) side_effect: SideEffect,
    pub(crate) evidence_role: EvidenceRole,
    pub(crate) cost_level: CostLevel,
    pub(crate) agent_auto_run: AutoRunPolicy,
    pub(crate) ci_allowed: bool,
    pub(crate) release_blocking: bool,
    pub(crate) output_contract: OutputContract,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionClass {
    Check,
    Validate,
    Audit,
    Gate,
    Secrets,
}

impl ActionClass {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Check => "check",
            Self::Validate => "validate",
            Self::Audit => "audit",
            Self::Gate => "gate",
            Self::Secrets => "secrets",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Domain {
    Agent,
    Repo,
    Security,
    Release,
}

impl Domain {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Repo => "repo",
            Self::Security => "security",
            Self::Release => "release",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SideEffect {
    Readonly,
    LocalWrite,
    SecretRead,
}

impl SideEffect {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Readonly => "readonly",
            Self::LocalWrite => "local_write",
            Self::SecretRead => "secret_read",
        }
    }

    pub(crate) fn is_high_risk(self) -> bool {
        matches!(self, Self::SecretRead)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EvidenceRole {
    Checked,
    ProvenCandidate,
}

impl EvidenceRole {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Checked => "checked",
            Self::ProvenCandidate => "proven_candidate",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CostLevel {
    Low,
    Medium,
    High,
}

impl CostLevel {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutoRunPolicy {
    Yes,
    Ask,
}

impl AutoRunPolicy {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::Ask => "ask",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputContract {
    Console,
    JsonReport(&'static str),
    Delegated,
}

impl OutputContract {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Console => "console",
            Self::JsonReport(_) => "json_report",
            Self::Delegated => "delegated",
        }
    }

    pub(crate) fn path(self) -> Option<&'static str> {
        match self {
            Self::JsonReport(path) => Some(path),
            Self::Console | Self::Delegated => None,
        }
    }
}

pub(crate) fn registry() -> &'static [ActionSpec] {
    &ACTION_REGISTRY
}

pub(crate) fn find_by_just_recipe(recipe: &str) -> Option<&'static ActionSpec> {
    registry()
        .iter()
        .find(|action| action.just_recipe == Some(recipe))
}

pub(crate) fn find_by_id(id: &str) -> Option<&'static ActionSpec> {
    registry().iter().find(|action| action.id == id)
}

pub(crate) fn suggested_justfile_path(action: &ActionSpec) -> &'static str {
    match (action.class, action.domain, action.side_effect) {
        (ActionClass::Validate, Domain::Agent, _) => "justfiles/check/agent.just",
        (ActionClass::Validate, Domain::Repo, _) => "justfiles/check/agent.just",
        (ActionClass::Audit, Domain::Repo, _) => "justfiles/maintain/audit.just",
        (ActionClass::Check, Domain::Repo, _) => "justfiles/check/architecture.just",
        (ActionClass::Secrets, Domain::Security, SideEffect::SecretRead) => {
            "justfiles/check/security.just"
        }
        (ActionClass::Gate, Domain::Release, _) => "justfiles/quality/gates.just",
        (ActionClass::Gate, _, _) => "justfiles/quality/gates.just",
        _ => "justfiles/_uncategorized.just",
    }
}

pub(crate) fn registry_invariant_issues() -> Vec<String> {
    let mut issues = Vec::new();
    for action in registry() {
        if action.side_effect.is_high_risk() && action.agent_auto_run == AutoRunPolicy::Yes {
            issues.push(format!(
                "{} has high-risk side effect {:?} but agent_auto_run is yes",
                action.id, action.side_effect
            ));
        }

        if action.canonical_cli.is_empty() {
            issues.push(format!("{} has empty canonical cli path", action.id));
        }

        if !action.legacy_cli_aliases.is_empty() {
            issues.push(format!(
                "{} declares legacy CLI aliases; use the public just facade or canonical CLI instead",
                action.id
            ));
        }

        if matches!(action.class, ActionClass::Audit) && action.output_contract.path().is_none() {
            issues.push(format!(
                "{} is an audit action without a stable output report path",
                action.id
            ));
        }

        if action.id.starts_with("deploy-") {
            issues.push(format!(
                "{} is a deploy action; register validate, dry-run, or smoke companion first",
                action.id
            ));
        }
    }
    issues
}

static ACTION_REGISTRY: [ActionSpec; 8] = [
    ActionSpec {
        id: "validate-agent-architecture",
        canonical_cli: &["agent", "validate", "architecture"],
        legacy_cli_aliases: &[],
        just_recipe: Some("validate-agent-architecture"),
        class: ActionClass::Validate,
        domain: Domain::Agent,
        side_effect: SideEffect::Readonly,
        evidence_role: EvidenceRole::Checked,
        cost_level: CostLevel::Low,
        agent_auto_run: AutoRunPolicy::Yes,
        ci_allowed: true,
        release_blocking: false,
        output_contract: OutputContract::Console,
    },
    ActionSpec {
        id: "validate-justfiles-taxonomy",
        canonical_cli: &["validate", "justfiles-taxonomy"],
        legacy_cli_aliases: &[],
        just_recipe: Some("validate-justfiles-taxonomy"),
        class: ActionClass::Validate,
        domain: Domain::Repo,
        side_effect: SideEffect::Readonly,
        evidence_role: EvidenceRole::Checked,
        cost_level: CostLevel::Low,
        agent_auto_run: AutoRunPolicy::Yes,
        ci_allowed: true,
        release_blocking: false,
        output_contract: OutputContract::Console,
    },
    ActionSpec {
        id: "audit-inventory",
        canonical_cli: &["audit", "inventory"],
        legacy_cli_aliases: &[],
        just_recipe: Some("audit-inventory"),
        class: ActionClass::Audit,
        domain: Domain::Repo,
        side_effect: SideEffect::LocalWrite,
        evidence_role: EvidenceRole::Checked,
        cost_level: CostLevel::Medium,
        agent_auto_run: AutoRunPolicy::Yes,
        ci_allowed: true,
        release_blocking: false,
        output_contract: OutputContract::JsonReport("target/audit/repo-inventory.json"),
    },
    ActionSpec {
        id: "audit-command-surface",
        canonical_cli: &["audit", "command-surface"],
        legacy_cli_aliases: &[],
        just_recipe: Some("audit-command-surface"),
        class: ActionClass::Audit,
        domain: Domain::Repo,
        side_effect: SideEffect::LocalWrite,
        evidence_role: EvidenceRole::Checked,
        cost_level: CostLevel::Low,
        agent_auto_run: AutoRunPolicy::Yes,
        ci_allowed: true,
        release_blocking: false,
        output_contract: OutputContract::JsonReport("target/audit/command-surface.json"),
    },
    ActionSpec {
        id: "boundary-check",
        canonical_cli: &["check", "boundaries"],
        legacy_cli_aliases: &[],
        just_recipe: Some("boundary-check"),
        class: ActionClass::Check,
        domain: Domain::Repo,
        side_effect: SideEffect::Readonly,
        evidence_role: EvidenceRole::Checked,
        cost_level: CostLevel::Low,
        agent_auto_run: AutoRunPolicy::Yes,
        ci_allowed: true,
        release_blocking: false,
        output_contract: OutputContract::Delegated,
    },
    ActionSpec {
        id: "sops-validate",
        canonical_cli: &["secrets", "validate"],
        legacy_cli_aliases: &[],
        just_recipe: Some("sops-validate"),
        class: ActionClass::Secrets,
        domain: Domain::Security,
        side_effect: SideEffect::SecretRead,
        evidence_role: EvidenceRole::Checked,
        cost_level: CostLevel::Low,
        agent_auto_run: AutoRunPolicy::Ask,
        ci_allowed: true,
        release_blocking: false,
        output_contract: OutputContract::Console,
    },
    ActionSpec {
        id: "gate-local",
        canonical_cli: &["gate", "local"],
        legacy_cli_aliases: &[],
        just_recipe: Some("gate-local"),
        class: ActionClass::Gate,
        domain: Domain::Repo,
        side_effect: SideEffect::Readonly,
        evidence_role: EvidenceRole::ProvenCandidate,
        cost_level: CostLevel::Medium,
        agent_auto_run: AutoRunPolicy::Ask,
        ci_allowed: false,
        release_blocking: false,
        output_contract: OutputContract::Delegated,
    },
    ActionSpec {
        id: "gate-ci",
        canonical_cli: &["gate", "ci"],
        legacy_cli_aliases: &[],
        just_recipe: Some("gate-ci"),
        class: ActionClass::Gate,
        domain: Domain::Release,
        side_effect: SideEffect::Readonly,
        evidence_role: EvidenceRole::ProvenCandidate,
        cost_level: CostLevel::High,
        agent_auto_run: AutoRunPolicy::Ask,
        ci_allowed: true,
        release_blocking: true,
        output_contract: OutputContract::Delegated,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_finds_actions_by_just_recipe() {
        let action = find_by_just_recipe("validate-justfiles-taxonomy").unwrap();

        assert_eq!(action.id, "validate-justfiles-taxonomy");
        assert_eq!(action.class, ActionClass::Validate);
        assert_eq!(action.side_effect, SideEffect::Readonly);
    }

    #[test]
    fn suggested_paths_follow_action_class_before_domain() {
        let action = find_by_id("audit-inventory").unwrap();

        assert_eq!(
            suggested_justfile_path(action),
            "justfiles/maintain/audit.just"
        );

        let action = find_by_id("sops-validate").unwrap();

        assert_eq!(
            suggested_justfile_path(action),
            "justfiles/check/security.just"
        );
    }

    #[test]
    fn high_risk_actions_are_not_default_auto_run() {
        assert!(registry_invariant_issues().is_empty());
    }
}
