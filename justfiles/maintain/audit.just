# Maintenance audits — local report generation for migration and inventory evidence.

# Machine-readable command surface audit for just/repo-tools migration planning
audit-command-surface:
    cargo run -p repo-tools -- audit command-surface

# Repository inventory audit for directory-level pass preparation
audit-inventory:
    cargo run -p repo-tools -- audit inventory
