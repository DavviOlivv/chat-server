#!/usr/bin/env bash
set -euo pipefail

# scripts/prepare-certs.sh
# copia /etc/ssl/certs/ca-certificates.crt para certs/ se existir
# Uso: ./scripts/prepare-certs.sh

SRC="/etc/ssl/certs/ca-certificates.crt"
DST_DIR="$(pwd)/certs"
DST_FILE="$DST_DIR/ca-certificates.crt"

if [ ! -f "$SRC" ]; then
  echo "ERROR: host cert bundle not found at $SRC"
  echo "On many systems the bundle exists. If it's in a different path, pass it as an argument:" \
       "./scripts/prepare-certs.sh /path/to/ca-certificates.crt"
  exit 1
fi

mkdir -p "$DST_DIR"
cp "$SRC" "$DST_FILE"
chmod 644 "$DST_FILE"

echo "Copied $SRC -> $DST_FILE"

echo "Remember to avoid committing certs/ to git (it's already in .gitignore)."
