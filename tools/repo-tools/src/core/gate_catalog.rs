use crate::cli::GateName;
use crate::support::Mode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GateSpec {
    pub(crate) name: GateName,
    pub(crate) id: &'static str,
    pub(crate) purpose: &'static str,
    pub(crate) default_mode: Mode,
    pub(crate) cost: GateCost,
    pub(crate) steps: &'static [GateStepSpec],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GateStepSpec {
    pub(crate) label: &'static str,
    pub(crate) command: CommandSpec,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommandSpec {
    Just(&'static [&'static str]),
    RepoTools(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GateCost {
    Low,
    Medium,
    High,
}

impl GateCost {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

impl CommandSpec {
    pub(crate) fn program(self) -> &'static str {
        match self {
            Self::Just(_) => "just",
            Self::RepoTools(_) => "cargo",
        }
    }

    pub(crate) fn args(self) -> Vec<String> {
        match self {
            Self::Just(args) => args.iter().map(|arg| (*arg).to_string()).collect(),
            Self::RepoTools(args) => ["run", "-p", "repo-tools", "--"]
                .into_iter()
                .chain(args.iter().copied())
                .map(ToOwned::to_owned)
                .collect(),
        }
    }
}

pub(crate) fn find_gate(name: GateName) -> &'static GateSpec {
    GATE_CATALOG
        .iter()
        .find(|gate| gate.name == name)
        .expect("all GateName variants must have a GateSpec")
}

pub(crate) fn gate_catalog() -> &'static [GateSpec] {
    &GATE_CATALOG
}

pub(crate) fn gate_catalog_invariant_issues() -> Vec<String> {
    let mut issues = Vec::new();
    for gate in gate_catalog() {
        if gate.id.is_empty() {
            issues.push("gate has empty id".to_string());
        }
        if gate.purpose.is_empty() {
            issues.push(format!("{} has empty purpose", gate.id));
        }
        if gate.steps.is_empty() {
            issues.push(format!("{} has no steps", gate.id));
        }
        if matches!(gate.cost, GateCost::High) && gate.default_mode != Mode::Strict {
            issues.push(format!(
                "{} is high cost but does not default to strict mode",
                gate.id
            ));
        }
    }
    issues
}

static LOCAL_STEPS: &[GateStepSpec] = &[
    GateStepSpec {
        label: "toolchain doctor",
        command: CommandSpec::Just(&["doctor"]),
    },
    GateStepSpec {
        label: "format check",
        command: CommandSpec::Just(&["fmt"]),
    },
    GateStepSpec {
        label: "lint",
        command: CommandSpec::Just(&["lint"]),
    },
];

static PREPUSH_STEPS: &[GateStepSpec] = &[
    GateStepSpec {
        label: "existence validation",
        command: CommandSpec::Just(&["gate-existence", "warn"]),
    },
    GateStepSpec {
        label: "import validation",
        command: CommandSpec::Just(&["gate-imports", "strict"]),
    },
    GateStepSpec {
        label: "publish intent validation",
        command: CommandSpec::Just(&["validate-publish-intent", "strict"]),
    },
    GateStepSpec {
        label: "typecheck",
        command: CommandSpec::Just(&["typecheck"]),
    },
    GateStepSpec {
        label: "unit test",
        command: CommandSpec::Just(&["test"]),
    },
    GateStepSpec {
        label: "platform validation",
        command: CommandSpec::Just(&["validate-platform"]),
    },
];

static CI_STEPS: &[GateStepSpec] = &[
    GateStepSpec {
        label: "full verify",
        command: CommandSpec::Just(&["verify"]),
    },
    GateStepSpec {
        label: "platform doctor",
        command: CommandSpec::Just(&["platform-doctor"]),
    },
    GateStepSpec {
        label: "validate state",
        command: CommandSpec::RepoTools(&["validate-state", "--mode", "strict"]),
    },
    GateStepSpec {
        label: "boundary check",
        command: CommandSpec::RepoTools(&["check", "boundaries"]),
    },
    GateStepSpec {
        label: "publish intent validation",
        command: CommandSpec::RepoTools(&["validate-publish-intent", "--mode", "strict"]),
    },
];

static RELEASE_STEPS: &[GateStepSpec] = &[
    GateStepSpec {
        label: "semver compatibility",
        command: CommandSpec::Just(&["semver-check", "", "$RELEASE_TYPE"]),
    },
    GateStepSpec {
        label: "contract drift",
        command: CommandSpec::Just(&["drift-check"]),
    },
    GateStepSpec {
        label: "backend-only audit",
        command: CommandSpec::Just(&["audit-backend-core", "strict"]),
    },
    GateStepSpec {
        label: "ci gate",
        command: CommandSpec::RepoTools(&["gate", "ci", "--mode", "strict"]),
    },
    GateStepSpec {
        label: "release build",
        command: CommandSpec::Just(&["build-release"]),
    },
];

static GATE_CATALOG: [GateSpec; 4] = [
    GateSpec {
        name: GateName::Local,
        id: "local",
        purpose: "low-cost local feedback before broader checks",
        default_mode: Mode::Warn,
        cost: GateCost::Low,
        steps: LOCAL_STEPS,
    },
    GateSpec {
        name: GateName::Prepush,
        id: "prepush",
        purpose: "developer pre-push guardrail for boundaries, imports, and tests",
        default_mode: Mode::Warn,
        cost: GateCost::Medium,
        steps: PREPUSH_STEPS,
    },
    GateSpec {
        name: GateName::Ci,
        id: "ci",
        purpose: "repo CI guardrail for backend-core and control-plane integrity",
        default_mode: Mode::Strict,
        cost: GateCost::High,
        steps: CI_STEPS,
    },
    GateSpec {
        name: GateName::Release,
        id: "release",
        purpose: "release readiness gate for compatibility, drift, CI, and build evidence",
        default_mode: Mode::Strict,
        cost: GateCost::High,
        steps: RELEASE_STEPS,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_gate_specs_are_well_formed() {
        assert!(gate_catalog_invariant_issues().is_empty());
    }

    #[test]
    fn ci_boundary_step_uses_canonical_cli() {
        let ci = find_gate(GateName::Ci);

        assert!(
            ci.steps
                .iter()
                .any(|step| { step.command == CommandSpec::RepoTools(&["check", "boundaries"]) })
        );
    }
}
