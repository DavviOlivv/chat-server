#!/usr/bin/env bash
set -euo pipefail

# Script para backup do banco de dados SQLite

DB_PATH="${DB_PATH:-users.db}"
BACKUP_DIR="${BACKUP_DIR:-backups}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="${BACKUP_DIR}/chat_backup_${TIMESTAMP}.db"

# Cria diretório de backups se não existir
mkdir -p "$BACKUP_DIR"

# Verifica se o banco existe
if [ ! -f "$DB_PATH" ]; then
    echo "❌ Banco de dados não encontrado: $DB_PATH"
    exit 1
fi

# Faz backup usando sqlite3
if command -v sqlite3 &> /dev/null; then
    echo "📦 Fazendo backup de $DB_PATH para $BACKUP_FILE..."
    sqlite3 "$DB_PATH" ".backup '$BACKUP_FILE'"
    echo "✅ Backup concluído: $BACKUP_FILE"
    
    # Mostra estatísticas
    SIZE=$(du -h "$BACKUP_FILE" | cut -f1)
    echo "📊 Tamanho do backup: $SIZE"
    
    # Lista últimos 5 backups
    echo ""
    echo "📋 Últimos backups:"
    ls -lh "$BACKUP_DIR" | tail -5
else
    # Fallback: copia arquivo diretamente
    echo "⚠️  sqlite3 não encontrado, usando cópia simples..."
    cp "$DB_PATH" "$BACKUP_FILE"
    echo "✅ Backup concluído: $BACKUP_FILE"
fi

# Opcional: limpar backups antigos (mantém últimos 10)
BACKUP_COUNT=$(ls -1 "$BACKUP_DIR" | wc -l)
if [ "$BACKUP_COUNT" -gt 10 ]; then
    echo "🧹 Limpando backups antigos (mantendo últimos 10)..."
    ls -t "$BACKUP_DIR"/chat_backup_*.db | tail -n +11 | xargs rm -f
fi
