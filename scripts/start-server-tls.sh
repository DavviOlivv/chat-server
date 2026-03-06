#!/usr/bin/env bash
set -euo pipefail

# Script para iniciar o servidor com TLS habilitado

# Gera certificados se não existirem
if [ ! -f "certs/cert.pem" ] || [ ! -f "certs/key.pem" ]; then
    echo "Certificados não encontrados, gerando..."
    ./scripts/gen-certs.sh
fi

echo "🚀 Iniciando servidor com TLS..."
export TLS_ENABLED=true
export TLS_CERT=certs/cert.pem
export TLS_KEY=certs/key.pem
export BIND_ADDR=0.0.0.0:8080
export RUST_LOG=info

cargo run --bin chat_server
