# Agent/control-plane checks — readonly metadata and command-surface validation.

# Agent architecture control-plane validation
validate-agent-architecture MODE='warn':
    cargo run -p repo-tools -- agent validate architecture --mode {{MODE}}

# Justfile/justfiles taxonomy validation
validate-justfiles-taxonomy MODE='warn':
    cargo run -p repo-tools -- validate justfiles-taxonomy --mode {{MODE}}

# Scoped strict admission for newly registered commands.
validate-new-command-taxonomy MODE='strict':
    cargo run -p repo-tools -- validate new-command-taxonomy --mode {{MODE}}
