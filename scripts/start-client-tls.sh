#!/usr/bin/env bash
set -euo pipefail

# Script para iniciar o cliente com TLS habilitado (modo inseguro para certificados autoassinados)

echo "🔒 Conectando ao servidor TLS (modo inseguro - aceita certificados autoassinados)..."
export TLS_ENABLED=true
export TLS_INSECURE=true
export SERVER_ADDR=127.0.0.1:8080
export RUST_LOG=info

cargo run --bin client
