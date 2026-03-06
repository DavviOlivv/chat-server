#!/usr/bin/env bash
set -euo pipefail

# Script para testar TLS end-to-end

echo "=== Teste TLS E2E ==="

# 1. Gerar certificados se necessário
if [ ! -f "certs/cert.pem" ] || [ ! -f "certs/key.pem" ]; then
    echo "📝 Gerando certificados..."
    ./scripts/gen-certs.sh
fi

# 2. Iniciar servidor TLS em background
echo "🚀 Iniciando servidor TLS..."
export TLS_ENABLED=true
export TLS_CERT=certs/cert.pem
export TLS_KEY=certs/key.pem
export BIND_ADDR=127.0.0.1:8080
export RUST_LOG=warn

cargo build --bin chat_server 2>&1 | grep -v "Compiling" || true
cargo run --bin chat_server > /tmp/chat-server-tls.log 2>&1 &
SERVER_PID=$!

echo "⏳ Aguardando servidor inicializar..."
sleep 2

# Verifica se o servidor está rodando
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo "❌ Servidor falhou ao iniciar. Log:"
    cat /tmp/chat-server-tls.log
    exit 1
fi

echo "✅ Servidor TLS rodando (PID: $SERVER_PID)"

# 3. Cleanup ao sair
trap "echo '🛑 Encerrando servidor...'; kill $SERVER_PID 2>/dev/null || true" EXIT

echo ""
echo "🔒 Servidor TLS rodando em 127.0.0.1:8080"
echo "Para testar, execute em outro terminal:"
echo "  ./scripts/start-client-tls.sh"
echo ""
echo "Ou manualmente:"
echo "  TLS_ENABLED=true TLS_INSECURE=true SERVER_ADDR=127.0.0.1:8080 cargo run --bin client"
echo ""
echo "Pressione Ctrl+C para encerrar o servidor."

# Mantém o script rodando
wait $SERVER_PID
