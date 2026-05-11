#!/bin/sh
# docker-entrypoint-gateway.sh
# Starts Pingora gateway in the container.

set -e

echo "[entrypoint] Starting Pingora gateway on ${BIND:-0.0.0.0:3000}"

exec /usr/local/bin/pingora-gateway
