# Template consumer commands

# Preview how this repository would be reduced for template consumers.
template-init profile='backend-core' mode='dry-run':
    cargo run -p repo-tools -- template-init --profile "{{profile}}" --mode "{{mode}}"

# Verify the default backend-core lane is independent from frontend repositories.
audit-backend-core mode='dry-run':
    cargo run -p repo-tools -- audit-backend-core "{{mode}}"

# Repository-level SemVer check for the current template release line.
semver-check baseline='' release_type='minor' release_tag_glob='v[0-9]*.[0-9]*.[0-9]*':
    cargo run -p repo-tools -- semver-check --baseline "{{baseline}}" --release-type "{{release_type}}" --release-tag-glob "{{release_tag_glob}}"
