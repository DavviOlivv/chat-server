#!/usr/bin/env bash
set -euo pipefail

# Script para query de estatísticas do banco de dados

DB_PATH="${DB_PATH:-users.db}"

if [ ! -f "$DB_PATH" ]; then
    echo "❌ Banco de dados não encontrado: $DB_PATH"
    exit 1
fi

echo "📊 Estatísticas do Banco de Dados"
echo "================================"
echo ""

# Total de usuários
USER_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM users;")
echo "👤 Usuários registrados: $USER_COUNT"

# Total de mensagens
MSG_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM messages;")
echo "💬 Mensagens armazenadas: $MSG_COUNT"

# Mensagens por tipo
echo ""
echo "📈 Mensagens por tipo:"
sqlite3 "$DB_PATH" "SELECT message_type, COUNT(*) as total FROM messages GROUP BY message_type;" \
    -header -column

# Top 5 usuários mais ativos
echo ""
echo "🏆 Top 5 usuários mais ativos:"
sqlite3 "$DB_PATH" "SELECT from_user, COUNT(*) as messages FROM messages GROUP BY from_user ORDER BY messages DESC LIMIT 5;" \
    -header -column

# Tamanho do banco
DB_SIZE=$(du -h "$DB_PATH" | cut -f1)
echo ""
echo "💾 Tamanho do banco: $DB_SIZE"
