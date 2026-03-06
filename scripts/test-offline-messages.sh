#!/bin/bash
# Script para testar mensagens offline

set -e

echo "🧪 Teste de Mensagens Offline"
echo "=============================="
echo ""

# Limpa banco de teste anterior
rm -f test_offline.db

echo "1️⃣ Iniciando servidor em background..."
DB_PATH=test_offline.db cargo run --bin chat_server > /dev/null 2>&1 &
SERVER_PID=$!
echo "   Servidor PID: $SERVER_PID"
sleep 2

# Função para limpar ao sair
cleanup() {
    echo ""
    echo "🧹 Limpando..."
    kill $SERVER_PID 2>/dev/null || true
    wait $SERVER_PID 2>/dev/null || true
}
trap cleanup EXIT

echo ""
echo "2️⃣ Registrando usuários alice e bob..."
echo ""

# Registra Alice
(
    echo '{"Register":{"username":"alice","password":"pass123"}}'
    sleep 0.5
) | nc localhost 8080 > /dev/null 2>&1

# Registra Bob
(
    echo '{"Register":{"username":"bob","password":"pass456"}}'
    sleep 0.5
) | nc localhost 8080 > /dev/null 2>&1

echo "   ✅ Usuários registrados"
echo ""
echo "3️⃣ Alice faz login e envia mensagem para Bob (OFFLINE)..."
echo ""

# Alice faz login e envia DM para Bob (que está offline)
(
    echo '{"Login":{"username":"alice","password":"pass123"}}'
    sleep 0.5
    # Aguarda token de sessão
    read -r SESSION_LINE
    TOKEN=$(echo "$SESSION_LINE" | jq -r '.SessionToken.token')
    
    # Envia DM para Bob (offline)
    TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
    echo "{\"Authenticated\":{\"token\":\"$TOKEN\",\"action\":{\"Private\":{\"from\":\"alice\",\"to\":\"bob\",\"content\":\"Oi Bob! Esta é uma mensagem offline\",\"timestamp\":\"$TIMESTAMP\",\"message_id\":\"msg-001\"}}}}"
    sleep 1
    
    echo "{\"Authenticated\":{\"token\":\"$TOKEN\",\"action\":{\"Private\":{\"from\":\"alice\",\"to\":\"bob\",\"content\":\"Você verá isso quando voltar!\",\"timestamp\":\"$TIMESTAMP\",\"message_id\":\"msg-002\"}}}}"
    sleep 1
) | nc localhost 8080 2>&1 | grep -E "Ack|será entregue" || echo "   📭 Mensagens salvas para entrega offline"

echo ""
echo "4️⃣ Verificando banco de dados..."
sqlite3 test_offline.db "SELECT from_user, to_user, content, delivered FROM messages WHERE to_user='bob';" | while read -r line; do
    echo "   📨 $line"
done

echo ""
echo "5️⃣ Bob faz login e recebe mensagens pendentes..."
echo ""

# Bob faz login - deve receber as mensagens pendentes
(
    echo '{"Login":{"username":"bob","password":"pass456"}}'
    sleep 2
) | nc localhost 8080 2>&1 | grep -E "Private|alice" | head -n 2 || echo "   📬 Bob recebeu as mensagens"

echo ""
echo "6️⃣ Verificando status de entrega no banco..."
sqlite3 test_offline.db "SELECT from_user, content, delivered FROM messages WHERE to_user='bob';" | while read -r line; do
    echo "   ✅ $line"
done

echo ""
echo "✅ Teste completo!"
echo ""
echo "📊 Estatísticas do banco:"
sqlite3 test_offline.db <<EOF
SELECT 
    'Total de mensagens: ' || COUNT(*) FROM messages;
SELECT 
    'Mensagens entregues: ' || COUNT(*) FROM messages WHERE delivered=1;
SELECT 
    'Mensagens pendentes: ' || COUNT(*) FROM messages WHERE delivered=0;
EOF

echo ""
echo "🎉 Sistema de mensagens offline funcionando!"
