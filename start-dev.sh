#!/usr/bin/env bash
# Run from the voidtower project root
set -e
cd "$(dirname "$0")"

VOIDTOWER_DATA_DIR=backend/dev-data \
VOIDTOWER_CONFIG_DIR=backend/dev-config \
VOIDTOWER_FRONTEND_DIR=frontend/dist \
VOIDTOWER_CATALOG_DIR=app-vault/apps \
backend/target/debug/voidtower "$@"
