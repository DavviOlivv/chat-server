#!/bin/bash
# Script para testar Full-Text Search (FTS5)

# Cores
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Teste FTS5 - Full-Text Search ===${NC}\n"

# Criar base de dados de teste com mensagens variadas
echo -e "${YELLOW}1. Populando banco com mensagens de teste...${NC}"

# Aguardar um pouco para garantir que o servidor está pronto
sleep 2

# Vamos usar o cliente TUI para enviar mensagens
# Criar um script expect para automatizar
cat > /tmp/test_fts5.exp << 'EOF'
#!/usr/bin/expect -f
set timeout 5

# Conectar ao servidor
spawn cargo run --bin client
expect "Digite seu username:"
send "alice\r"
expect "Digite sua senha:"
send "pass123\r"
sleep 1

# Enviar mensagens variadas para popular o banco
send "Hello world! This is my first message\r"
sleep 0.5
send "Rust is amazing for systems programming\r"
sleep 0.5
send "Tokio is a great async runtime for Rust\r"
sleep 0.5
send "Learning async/await in Rust today\r"
sleep 0.5
send "Building a chat server with tokio\r"
sleep 0.5
send "/msg bob Hey Bob, let's talk about Rust!\r"
sleep 0.5
send "SQLite FTS5 provides full-text search\r"
sleep 0.5
send "Database indexing is really fast\r"
sleep 0.5
send "The quick brown fox jumps over the lazy dog\r"
sleep 0.5
send "Async programming in Rust is powerful\r"
sleep 0.5

# Agora vamos testar buscas
send "/search rust\r"
sleep 1
send "/search tokio AND async\r"
sleep 1
send "/search \"async runtime\"\r"
sleep 1
send "/search NEAR(rust tokio, 5)\r"
sleep 1
send "/search rust from:alice\r"
sleep 1

sleep 2
send "\x03"
expect eof
EOF

chmod +x /tmp/test_fts5.exp

echo -e "${GREEN}✓ Script de teste criado${NC}\n"

echo -e "${YELLOW}2. Executando testes...${NC}"
echo -e "${BLUE}OBS: Certifique-se de que o servidor está rodando!${NC}\n"

# Executar o script
/tmp/test_fts5.exp

echo -e "\n${GREEN}=== Testes Concluídos ===${NC}"
echo -e "${YELLOW}Verifique os resultados acima para validar:${NC}"
echo "  1. Mensagens foram inseridas"
echo "  2. Busca simples: /search rust"
echo "  3. Boolean AND: /search tokio AND async"
echo "  4. Frase exata: /search \"async runtime\""
echo "  5. Proximidade: /search NEAR(rust tokio, 5)"
echo "  6. Filtro usuário: /search rust from:alice"
echo ""
echo -e "${BLUE}Para testes manuais, use:${NC}"
echo "  cargo run --bin tui"
echo "  Depois digite comandos como:"
echo "    /search rust"
echo "    /search tokio AND async"
echo "    /search \"async runtime\""
