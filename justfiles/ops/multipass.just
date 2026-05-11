# Multipass + K3s profile — documented pre-1.0 rehearsal only.
#
# This lane is intentionally not used by default on an 8GB/256GB Mac mini.

multipass-k3s-plan:
    @printf "Multipass + K3s is documented for the pre-1.0 manual rehearsal.\n"
    @printf "Read: docs/operations/advanced-topology/k3s-local-multipass.md\n"
    @printf "Resource target: k3s-1 2CPU/2GB/15GB, k3s-2 1CPU/1GB/10GB, k3s-3 1CPU/1GB/10GB.\n"
    @printf "Do not run Colima/K3d and Multipass at the same time on the 8GB Mac mini.\n"
