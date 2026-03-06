# chat-serve — Guia Completo

![build](https://img.shields.io/badge/build-passing-brightgreen) ![tests](https://img.shields.io/badge/tests-28%20passed-brightgreen) ![license](https://img.shields.io/badge/license-MIT-blue) ![fts5](https://img.shields.io/badge/FTS5-enabled-orange) ![admin](https://img.shields.io/badge/admin-moderation-red) ![files](https://img.shields.io/badge/files-transfer-purple)

Este documento é a referência técnica e operacional do projeto `chat-serve`. Ele explica a arquitetura, decisões de design, como rodar, testar, empacotar em Docker, observabilidade, e comandos úteis para desenvolvimento e CI.

## ✨ Destaques

- 🔍 **Full-Text Search (FTS5)**: Busca instantânea com O(log n), ranking por relevância, snippets destacados
- 🛡️ **Sistema Admin**: Moderação completa (kick, ban, mute, promote, logs)
- 📎 **Transferência de Arquivos**: HTTP desacoplado do chat, tokens temporários, streaming
- 🌐 **WebSocket Gateway**: Suporte para clientes web sem modificar servidor
- 🎨 **Cliente TUI Moderno**: Interface rica com abas, filtros, notificações
- 🔒 **Autenticação Bcrypt**: Login/registro seguro com tokens de sessão
- 💾 **Persistência SQLite**: Mensagens, usuários, moderação com triggers automáticos

## 🚀 Quick Start

```bash
# Servidor
cargo run --bin chat_server

# Cliente Original (CLI)
cargo run --bin client

# Cliente TUI (Interface Moderna) ⭐ NOVO
cargo run --bin tui

# WebSocket Gateway (Clientes Web) 🌐 NOVO
cargo run --bin ws_gateway
# Depois abra: examples/web_client.html no navegador

# File Server (Transferência de Arquivos) 📎 NOVO
cargo run --bin file_server
```

Documentação completa: [QUICKSTART_TUI.md](QUICKSTART_TUI.md) | [WEBSOCKET_GATEWAY.md](WEBSOCKET_GATEWAY.md) | [FTS5_SEARCH.md](docs/FTS5_SEARCH.md) | [FILE_TRANSFER.md](docs/FILE_TRANSFER.md)

## 📦 Clientes Disponíveis

### Cliente Original (`client`)
- Interface CLI básica (stdout/stdin)
- Comandos: `/msg`, `/list`, `/join`, `/leave`
- Bom para scripts e automação

### Cliente TUI (`tui`) ⭐ **Recomendado para Terminal**
- Interface moderna com [Ratatui](https://ratatui.rs/)
- **3 Abas**: Geral, DMs, Salas
- **Filtros Inteligentes**: Mensagens organizadas por tipo
- **Navegação**: PageUp/PageDown, Auto-scroll
- **Notificações**: Contadores de não lidas, alertas visuais
- **Typing Indicators**: Veja quem está digitando em tempo real
- **Cores Personalizáveis**: Configure via `colors.toml`
- **Painel de Usuários**: Lista sempre visível

📖 Documentação: [TUI_CLIENT.md](TUI_CLIENT.md)  
🎯 Exemplos: [TUI_EXAMPLE.md](TUI_EXAMPLE.md)

### Cliente Web (`web_client.html`) 🌐 **Recomendado para Browser**
- Interface HTML/CSS/JavaScript moderna
- **WebSocket Gateway**: Conecta via `ws://localhost:8081`
- **UI Responsiva**: Design com gradientes e animações
- **Todas as features**: DMs, Salas, Typing, Notificações
- **Zero instalação**: Basta abrir no navegador
- **Cross-platform**: Funciona em qualquer SO com browser
- **Mobile-ready**: Responsivo para smartphones

📖 Documentação: [WEBSOCKET_GATEWAY.md](WEBSOCKET_GATEWAY.md)

## 🌐 Arquitetura WebSocket

```
Browser ──WebSocket──► Gateway (8081) ──TCP──► Servidor (8080)
   ▲                         │                       │
   │                         │                       │
   └─────────JSON──────────Bridge──────JSON─────────┘
```

O WebSocket Gateway atua como **proxy transparente**, permitindo clientes web sem modificar o servidor TCP existente.

## Sumário
- Visão Geral
- Autenticação
- Segurança TLS
- Persistência SQLite
- Clientes (CLI e TUI)
- Exemplos JSON
- Arquitetura e Modelo de Dados
- Decisões de Design e Concurrency
- Protocolos e formato das mensagens
- Testes e Qualidade
- Logging e Observabilidade
- Docker: imagens, variantes e comandos
- Makefile: targets úteis
- Desenvolvimento local (fluxos rápidos)
- CI / Deploy sugestões
- Arquivos importantes e próximos passos

---

Visão Geral
---------
`chat-serve` é um servidor/cliente de chat de exemplo escrito em Rust (Tokio, serde/json). O projeto tem:
- Uma crate library (`chat_serve`) com modelos e utilitários.
- **Dois clientes**: CLI tradicional e TUI moderno com Ratatui

## 🔍 Full-Text Search (FTS5) ⚡️ NOVO

Busca instantânea de mensagens com **SQLite FTS5** (Full-Text Search 5):

### Características

- **Performance O(log n)**: 100-1000x mais rápido que LIKE queries
- **Ranking por Relevância**: Resultados ordenados por score
- **Snippets Destacados**: Palavras-chave marcadas automaticamente
- **Operadores Booleanos**: AND, OR, NOT, frases, proximidade
- **Auto-indexação**: Triggers mantêm índice sincronizado
- **100-500 buscas/segundo**: Performance em produção

### Uso

```bash
# Busca simples
/search rust

# Operadores booleanos
/search tokio AND async
/search chat OR server
/search rust NOT async

# Busca por frase exata
/search "async runtime"

# Proximidade (palavras dentro de N tokens)
/search NEAR(rust tokio, 5)

# Filtrar por usuário
/search rust from:alice
```

### Exemplo de Resposta

```json
{
  "SearchResponse": {
    "messages": [{
      "id": 42,
      "from_user": "alice",
      "content": "Tokio is great for async Rust",
      "timestamp": "2024-01-15 10:30:00",
      "rank": -1.23,
      "snippet": "**Tokio** is great for async **Rust**"
    }],
    "total": 1
  }
}
```

📖 **Documentação Completa**: [docs/FTS5_SEARCH.md](docs/FTS5_SEARCH.md)  
🏗️ **Detalhes Técnicos**: [docs/FTS5_IMPLEMENTATION.md](docs/FTS5_IMPLEMENTATION.md)

## 🛡️ Sistema de Moderação (Admin)

Sistema completo de moderação com 8 comandos administrativos:

### Comandos

```bash
/admin kick <user>              # Expulsar usuário
/admin ban <user> [tempo] [msg] # Banir (temp/permanente)
/admin mute <user> [tempo]      # Silenciar usuário
/admin unmute <user>            # Remover silenciamento
/admin promote <user>           # Promover a admin
/admin demote <user>            # Remover admin
/admin list                     # Listar admins
/admin logs [N]                 # Ver logs de moderação
```

### Funcionalidades

- ✅ Bans temporários (5m, 2h, 1d) ou permanentes
- ✅ Mutes temporários com auto-expiração
- ✅ Logs de moderação imutáveis (auditoria)
- ✅ Verificação de ban no login
- ✅ Verificação de mute ao enviar mensagens
- ✅ 4 tabelas SQLite: admins, bans, mutes, moderation_logs

## 📎 Transferência de Arquivos

Sistema de transferência de arquivos seguindo **padrões da indústria**: desacoplamento entre canal de controle (chat TCP) e canal de dados (HTTP).

### Arquitetura

```
Cliente ──TCP (8080)──► Chat Server ◄──► Database
   │                     (Controle)        (SQLite)
   │
   └──HTTP (3001)──► File Server ◄──► Storage
                      (Dados)           (uploads/)
```

### Por quê essa abordagem?

1. **Desacoplamento**: Chat TCP otimizado para mensagens curtas, HTTP para arquivos grandes
2. **Eficiência**: Streaming com zero-copy, sem bloquear o chat
3. **Escalabilidade**: Fácil migração para S3/MinIO

### Fluxo de Upload/Envio

1. Cliente faz POST /upload com metadados → recebe `file_id`
2. Cliente faz POST /upload/:file_id com bytes do arquivo
3. Cliente envia `ChatAction::SendFile` via chat
4. Server gera token temporário (1h) para destinatário
5. Destinatário recebe `FileNotification` com token
6. Destinatário faz GET /download/:file_id/:token

### Segurança

- ✅ Tokens temporários (1 hora de validade)
- ✅ Validação de permissões (apenas uploader/destinatário)
- ✅ Path traversal protection (UUID, não filename)
- ✅ SQL injection protection (queries parametrizadas)

### Iniciar File Server

```bash
RUST_LOG=info FILE_SERVER_PORT=3001 cargo run --bin file_server
```

📖 **Documentação Completa**: [docs/FILE_TRANSFER.md](docs/FILE_TRANSFER.md)

Autenticação (novo)
-------------------
O sistema de autenticação foi adicionado na forma "Fase 1" (registro/login com bcrypt e tokens de sessão). Resumo rápido:

- `Login { username, password }` — credenciais enviadas pelo cliente.
- `Register { username, password }` — registra novo usuário (servidor responde com `Ack::System` ou `Ack::Failed`).
- `SessionToken { token, username }` — resposta do servidor após login bem-sucedido.
- `Authenticated { token, action }` — envelope que deve envolver qualquer `ChatAction` autenticada (subtipo do protocolo).

Exemplo — Login (cliente -> servidor):

```json
{"type":"Login","username":"alice","password":"senha123"}
```

Exemplo — Resposta com token (servidor -> cliente):

```json
{"type":"SessionToken","token":"550e8400-e29b-41d4-a716-446655440000","username":"alice"}
```

Exemplo — Envelope autenticado ao enviar uma DM (cliente -> servidor):

```json
{
  "type": "Authenticated",
  "token": "550e8400-e29b-41d4-a716-446655440000",
  "action": {
    "type": "Private",
    "from": "alice",
    "to": "bob",
    "content": "Oi Bob!",
    "timestamp": "2026-03-04T12:00:00Z",
    "message_id": "550e8400-e29b-41d4-a716-446655440001"
  }
}
```

Notas de implementação:

- As senhas são hasheadas com `bcrypt` (custo 12) no servidor antes de persistir.
- O servidor persiste usuários em JSON no arquivo configurável via `USERS_FILE` (padrão `users.json`) usando escrita atômica: grava em `users.json.tmp` e renomeia para `users.json` para evitar corrupção em falhas.
- O servidor responde a operações de autenticação com `Ack::System` em sucesso e `Ack::Failed` em caso de erro (por exemplo, credenciais inválidas ou username já existente).
- O cliente de linha de comando já suporta `/register` (usa as credenciais digitadas no começo) e automaticamente passa a encapsular ações em `Authenticated` quando recebe um `SessionToken`.
 - O cliente de linha de comando já suporta `/register` (usa as credenciais digitadas no começo) e automaticamente passa a encapsular ações em `Authenticated` quando recebe um `SessionToken`.
 - O cliente lê a senha sem eco no terminal usando o crate `rpassword` para melhor segurança/UX.
 - Se o servidor responder com um `Ack::Failed` indicando `Token inválido ou expirado`, o cliente limpa o `session_token` local automaticamente e pede que o usuário faça login novamente.
 - Expiração de tokens e cleanup:
   - Tokens de sessão têm TTL configurável via `TOKEN_TTL_SECS` (padrão 86400s = 24h). O método `validate_token` verifica a expiração e remove tokens expirados.
   - Há um loop de limpeza periódico (quando um runtime Tokio está ativo) que remove tokens expirados a cada `TOKEN_CLEANUP_INTERVAL_SECS` (default 60s). Para evitar deadlocks o loop coleta chaves expiradas antes de removê-las.
   - Em ambientes de teste o cleanup automático pode ser desabilitado definindo `SKIP_TOKEN_CLEANUP=1` e os testes devem validar a expiração chamando `validate_token` diretamente.
 - Variáveis de ambiente relacionadas à autenticação/segurança:
   - `USERS_FILE` — caminho do arquivo JSON de usuários (padrão `users.json`).
   - `BCRYPT_COST` — custo do `bcrypt` (default 12). Para testes, reduza (ex.: `BCRYPT_COST=4`) para acelerar hashing.
   - `TOKEN_TTL_SECS` — tempo de vida do token em segundos (default 86400).
   - `TOKEN_CLEANUP_INTERVAL_SECS` — intervalo em segundos para o loop de limpeza (default 60).
   - `SKIP_TOKEN_CLEANUP` — se definido, o loop de limpeza não é iniciado (útil em testes).
- Exemplo: executar com arquivo de usuários personalizado
------------------------------------------------------
Você pode configurar o caminho do arquivo de usuários via variável de ambiente `USERS_FILE`. Exemplo para rodar o servidor usando `dev_users.json`:

```bash
USERS_FILE=dev_users.json RUST_LOG=info cargo run --bin chat_server
```

Se o arquivo não existir, ele será criado no primeiro registro. O servidor escreve de forma atômica no arquivo (`.tmp` + `rename`).

Fluxo rápido: `/register` então `login`
------------------------------------
Abaixo um exemplo passo-a-passo (payloads JSON) de um cliente que tenta logar, registra-se e loga de novo.

1) Cliente -> servidor: Login (usuário ainda não existe)

```json
{"type":"Login","username":"alice","password":"senha123"}
```

2) Servidor -> cliente: Ack Failed (usuário não encontrado)

```json
{"type":"Ack","kind":"Failed","info":"Usuário 'alice' não encontrado","message_id":null}
```

3) Cliente -> servidor: Register

```json
{"type":"Register","username":"alice","password":"senha123"}
```

4) Servidor -> cliente: Ack System (registro ok)

```json
{"type":"Ack","kind":"System","info":"Registro realizado com sucesso","message_id":null}
```

5) Cliente -> servidor: Login (novamente)

```json
{"type":"Login","username":"alice","password":"senha123"}
```

6) Servidor -> cliente: SessionToken + Ack System

```json
{"type":"SessionToken","token":"550e8400-e29b-41d4-a716-446655440000","username":"alice"}
```

```json
{"type":"Ack","kind":"System","info":"Login bem-sucedido","message_id":null}
```

Depois de receber o `SessionToken` o cliente pode enviar ações autenticadas usando o envelope `Authenticated { token, action }`.

Segurança TLS (Transport Layer Security)
---------------------------------------
O chat-serve suporta TLS/SSL para criptografar a comunicação entre cliente e servidor, usando a biblioteca `rustls` (moderna e segura, sem dependência do OpenSSL).

### 🔒 Por que TLS?

TLS adiciona criptografia ponta-a-ponta, protegendo:
- Credenciais de login (username/password) contra interceptação
- Mensagens privadas (DMs) e broadcasts
- Tokens de sessão contra roubo (session hijacking)

### 📋 Configuração Rápida

1. **Gerar certificados autoassinados** (desenvolvimento):

```bash
./scripts/gen-certs.sh
```

Isso cria `certs/cert.pem` e `certs/key.pem` válidos por 365 dias para `localhost`.

2. **Iniciar servidor com TLS**:

```bash
./scripts/start-server-tls.sh
```

Ou manualmente:

```bash
export TLS_ENABLED=true
export TLS_CERT=certs/cert.pem
export TLS_KEY=certs/key.pem
export BIND_ADDR=0.0.0.0:8080
cargo run --bin chat_server
```

3. **Conectar cliente com TLS**:

```bash
./scripts/start-client-tls.sh
```

Ou manualmente:

```bash
export TLS_ENABLED=true
export TLS_INSECURE=true  # Aceita certificados autoassinados
export SERVER_ADDR=127.0.0.1:8080
cargo run --bin client
```

### ⚙️ Variáveis de Ambiente TLS

**Servidor:**
- `TLS_ENABLED` — habilita TLS (`true` ou `1`; padrão: `false`)
- `TLS_CERT` — caminho do certificado (padrão: `certs/cert.pem`)
- `TLS_KEY` — caminho da chave privada (padrão: `certs/key.pem`)

**Cliente:**
- `TLS_ENABLED` — habilita TLS (`true` ou `1`; padrão: `false`)
- `TLS_INSECURE` — aceita certificados autoassinados sem validação (`true` ou `1`; **APENAS para desenvolvimento**)
- `TLS_CA_CERT` — caminho do certificado CA personalizado (opcional; se omitido usa CAs do sistema)
- `SERVER_ADDR` — endereço do servidor (padrão: `127.0.0.1:8080`)

### ⚠️ Modo Inseguro (Desenvolvimento)

Para facilitar testes locais com certificados autoassinados, use `TLS_INSECURE=true` no cliente. Isso **desabilita a validação de certificados** e deve ser usado **APENAS em desenvolvimento**.

Para produção:
1. Use certificados válidos de uma CA reconhecida (Let's Encrypt, etc.)
2. Configure o cliente com `TLS_CA_CERT` apontando para o CA correto
3. Remova `TLS_INSECURE` (deixe `false` ou omitido)

### 🧪 Teste End-to-End

Execute o script de teste automático que inicia o servidor TLS e aguarda conexões:

```bash
./scripts/test-tls.sh
```

Em outro terminal, conecte um cliente:

```bash
./scripts/start-client-tls.sh
```

### 📈 Performance

O TLS adiciona overhead mínimo no handshake inicial (~1-2ms). O `rustls` é altamente otimizado e o servidor reutiliza o `TlsAcceptor` (é um `Arc` internamente), evitando reprocessamento de certificados.

### 🛡️ Produção: Certificados Válidos

Para produção, gere certificados válidos:

**Opção 1: Let's Encrypt (recomendado para servidores públicos)**
```bash
# Instale certbot e obtenha certificados gratuitos
certbot certonly --standalone -d seu-dominio.com
```

**Opção 2: Certificados corporativos**
- Use os certificados fornecidos pela sua organização
- Configure `TLS_CERT` e `TLS_KEY` para apontar para os arquivos

**Exemplo de inicialização em produção:**
```bash
export TLS_ENABLED=true
export TLS_CERT=/etc/letsencrypt/live/chat.exemplo.com/fullchain.pem
export TLS_KEY=/etc/letsencrypt/live/chat.exemplo.com/privkey.pem
export BIND_ADDR=0.0.0.0:443
cargo run --release --bin chat_server
```

### 🔍 Validação de Certificados

O cliente valida certificados por padrão usando as CAs do sistema. Para usar uma CA personalizada:

```bash
export TLS_CA_CERT=certs/my-ca.pem
export TLS_ENABLED=true
cargo run --bin client
```

### 📚 Scripts Disponíveis

- `scripts/gen-certs.sh` — gera certificados autoassinados em `certs/`
- `scripts/start-server-tls.sh` — inicia servidor com TLS
- `scripts/start-client-tls.sh` — inicia cliente com TLS (modo inseguro)
- `scripts/test-tls.sh` — teste E2E automatizado

Persistência SQLite
------------------
O chat-serve usa SQLite para armazenar usuários e mensagens de forma persistente, com migrações automáticas e backups simples.

### 📦 O que é persistido?

**Usuários:**
- Username (chave primária)
- Password hash (bcrypt)
- Data de criação

**Mensagens:**
- ID autoincremental
- Remetente (from_user)
- Destinatário (to_user) — NULL para broadcasts
- Sala (room) — NULL para DMs
- Conteúdo
- Timestamp
- Message ID (UUID, para deduplicação)
- Tipo (private/broadcast/room)
- **Status de entrega (delivered)** — para mensagens offline

### 📭 Mensagens Offline (novo)

O servidor implementa um sistema completo de **queue para entrega posterior** de mensagens diretas:

**Como funciona:**
1. Alice envia DM para Bob (que está offline)
2. Servidor salva a mensagem com `delivered = 0`
3. Alice recebe ACK: "Mensagem será entregue quando voltar online"
4. Bob faz login → Servidor envia notificação: "📬 Você tem 3 mensagens novas recebidas enquanto estava fora:"
5. Servidor envia todas as mensagens automaticamente para Bob
6. Mensagens marcadas como `delivered = 1` após envio

**Características:**
- ✅ **Zero perda de mensagens** — tudo salvo no banco
- ✅ **Entrega automática** — no momento do login
- ✅ **Notificação contextual** — usuário sabe quantas mensagens esperar
- ✅ **Separação visual** — bordas decorativas para melhor UX
- ✅ **Performance otimizada** — índice `(to_user, delivered)` para busca O(log n)
- ✅ **Verificação de usuário** — não salva para usuários inexistentes
- ✅ **Ordem cronológica** — mensagens entregues na ordem de envio

**Teste:**
```bash
./scripts/test-offline-messages.sh
```

**Documentação completa:** 
- [OFFLINE_MESSAGES.md](OFFLINE_MESSAGES.md) - Arquitetura e implementação técnica
- [OFFLINE_UX_EXAMPLE.md](OFFLINE_UX_EXAMPLE.md) - Exemplos visuais e experiência do usuário

### 🔄 Migração Automática

Se existir um arquivo `users.json` legado, o servidor migra automaticamente para SQLite na primeira inicialização e cria um backup:

```bash
# Ao iniciar o servidor
cargo run --bin chat_server

# Logs:
# 🔄 Migrados 5 usuários de users.json para SQLite
# 📦 Backup do JSON criado em users.json.backup
```

### ⚙️ Configuração

**Variáveis de ambiente:**
- `DB_PATH` — caminho do banco SQLite (padrão: deriva de `USERS_FILE`, ex: `users.db`)
- `USERS_FILE` — arquivo JSON legado (padrão: `users.json`)

**Exemplo:**
```bash
export DB_PATH=chat_production.db
export USERS_FILE=users.json  # Para migração
cargo run --bin chat_server
```

### 🧪 Testes

Os testes usam banco em memória (`:memory:`) automaticamente, garantindo isolamento e velocidade:

```bash
cargo test
# Todos os testes passam com SQLite em memória
```

### 📊 Estatísticas e Queries

Use o script `db-stats.sh` para ver estatísticas do banco:

```bash
./scripts/db-stats.sh
```

**Saída exemplo:**
```
📊 Estatísticas do Banco de Dados
================================

👤 Usuários registrados: 12
💬 Mensagens armazenadas: 1543

📈 Mensagens por tipo:
message_type  total
------------  -----
broadcast     234
private       1127
room          182

🏆 Top 5 usuários mais ativos:
from_user  messages
---------  --------
alice      456
bob        321
charlie    198

💾 Tamanho do banco: 512K
```

### 💾 Backup e Restore

**Backup automático:**
```bash
./scripts/backup-db.sh
```

Cria backup timestamped em `backups/chat_backup_YYYYMMDD_HHMMSS.db` e mantém os últimos 10 automaticamente.

**Restore manual:**
```bash
# Para de servidor
pkill chat_server

# Restaura do backup
cp backups/chat_backup_20260305_120000.db users.db

# Reinicia servidor
cargo run --bin chat_server
```

**Agendamento de backups (cron):**
```bash
# Adicione ao crontab para backup diário às 3h
0 3 * * * /path/to/chat-serve/scripts/backup-db.sh
```

### 🔍 Queries Manuais

**Usando sqlite3 CLI:**
```bash
# Conectar ao banco
sqlite3 users.db

# Ver usuários
SELECT * FROM users;

# Ver mensagens recentes
SELECT from_user, content, timestamp 
FROM messages 
ORDER BY timestamp DESC 
LIMIT 10;

# Buscar DMs entre dois usuários
SELECT from_user, to_user, content, timestamp
FROM messages
WHERE (from_user = 'alice' AND to_user = 'bob')
   OR (from_user = 'bob' AND to_user = 'alice')
ORDER BY timestamp;

# Mensagens de uma sala
SELECT from_user, content, timestamp
FROM messages
WHERE room = '#general'
ORDER BY timestamp DESC
LIMIT 20;
```

### 🗄️ Schema do Banco

**Tabela users:**
```sql
CREATE TABLE users (
    username TEXT PRIMARY KEY NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TEXT NOT NULL
);
```

**Tabela messages:**
```sql
CREATE TABLE messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_user TEXT NOT NULL,
    to_user TEXT,
    room TEXT,
    content TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    message_id TEXT UNIQUE,
    message_type TEXT NOT NULL
);

-- Índices para performance
CREATE INDEX idx_messages_from ON messages(from_user);
CREATE INDEX idx_messages_to ON messages(to_user);
CREATE INDEX idx_messages_room ON messages(room);
CREATE INDEX idx_messages_timestamp ON messages(timestamp);
```

### 🚀 Performance

- **Tamanho:** ~500 bytes por mensagem, ~200 bytes por usuário
- **Velocidade:** Insert ~10K msgs/s, Query ~100K msgs/s (SSD)
- **Escala:** SQLite suporta até 140 TB de dados
- **Concorrência:** WAL mode ativado (múltiplos leitores, 1 escritor)

### 🛡️ Segurança e GDPR

**Limpeza de dados antigos:**
```rust
// No código: Database::delete_messages_older_than(days: i64)
// Exemplo: deletar mensagens com mais de 90 dias
db.delete_messages_older_than(90)?;
```

**Anonimização:**
```sql
-- Anonimizar mensagens de um usuário (GDPR)
UPDATE messages SET from_user = 'deleted_user' WHERE from_user = 'alice';
DELETE FROM users WHERE username = 'alice';
```

### 📁 Scripts Disponíveis

- `scripts/backup-db.sh` — backup automático com rotação
- `scripts/db-stats.sh` — estatísticas e análises

- Binários:
  - `chat_server` — servidor TCP que aceita clientes JSON-por-linha.
  - `client` — cliente interativo (linha de comando) com UI colorida.

O protocolo é simples: cada linha TCP contém um objeto JSON representando `ChatMessage`.

Quickstart (3 comandos)
-----------------------
Seguem três comandos mínimos para começar localmente:

```bash
cargo build
RUST_LOG=info cargo run --bin chat_server
cargo run --bin client
```

Por padrão o README usa a porta 8080 nos exemplos — ver `Makefile` ou `src/main.rs` para alterar.

Exemplos JSON
-------------
Abaixo estão exemplos reais de payloads JSON que o servidor/cliente trocam (uma linha por mensagem):

Login:

```json
{"type":"Login","username":"alice"}
```

Mensagem privada (DM) com `message_id` (UUID):

```json
{"type":"Private","from":"alice","to":"bob","content":"Oi Bob!","timestamp":"2026-03-04T12:00:00Z","message_id":"550e8400-e29b-41d4-a716-446655440000"}
```

Ack (exemplo de ACK correlacionando `message_id`):

```json
{"type":"Ack","kind":"Delivered","info":"entregue ao cliente","message_id":"550e8400-e29b-41d4-a716-446655440000"}
```


Arquitetura e Modelo de Dados
----------------------------
Componentes principais:
- `ChatState` — estado compartilhado do servidor (mapa de clientes, histórico, dedup de message_id).
- `ChatCore` — lógica de alto nível (register, broadcast, enviar DM, processar ACKs, /list).
- `Client handler` — per-connection tasks que fazem parsing/serialização JSON e conversam com `ChatCore`.

Modelo de mensagem (`ChatMessage`) (resumo):
- `Login { username }`
- `Text { from, content, timestamp }` — broadcast público
- `Private { from, to, content, timestamp, message_id }` — DM; `message_id` é Option<String> (UUID)
- `Ack { kind: AckKind, info: String, message_id: Option<String> }`
- `ListRequest { from }` / `ListResponse { users }`
- `Error(String)`

`AckKind` enum: `Received`, `Delivered`, `Read`, `Failed`, `System`.

Semântica dos ACKs (incluindo rate limiting)
-------------------------------------------
- `AckKind::Delivered` — usado para confirmar DMs (`Private`) entregues com sucesso. O campo `message_id` é copiado da mensagem original para permitir correlacionar retries no cliente.
- `AckKind::Failed` — indica falha lógica de processamento, por exemplo:
  - Envios de `RoomText` para uma sala na qual o remetente não está inscrito.
  - Violação de rate limiting por usuário (limite de mensagens por segundo).
- `AckKind::System` — usado para mensagens de sistema (ex.: confirmação de `JoinRoom`/`LeaveRoom`, mensagens informativas).

Rate limiting por usuário
-------------------------
O servidor implementa um rate limiting básico por usuário, com janelas de 1 segundo:
- Limite padrão: **10 mensagens/segundo** por usuário (ajustável via `RATE_LIMIT_PER_SEC`).
- O limite é aplicado a mensagens `Text`, `RoomText`, `Private`, `ListRequest`, `JoinRoom` e `LeaveRoom`.
- Quando um usuário excede o limite na janela atual, o servidor **não processa** a mensagem e envia de volta um `Ack` com `kind = Failed`:

```json
{"type":"Ack","kind":"Failed","info":"Limite de mensagens excedido (10 msg/s).","message_id":null}
```

- Para mensagens `Private` com `message_id`, o mesmo `message_id` é reaproveitado no `Ack::Failed`, permitindo que o cliente marque aquela tentativa específica como falha definitiva e cancele novos retries.
  O cliente de linha de comando detecta essa mensagem e exibe um aviso extra em amarelo informando que você está enviando mensagens rápido demais e deve aguardar um pouco antes de tentar novamente.

Decisões de Design e Concorrência
---------------------------------
- Linguagem: Rust para segurança de memória e concorrência.
- Runtime: Tokio para I/O assíncrono.
- Concurrency: uso de `dashmap` para `ChatState.clients` e `seen_message_ids` (leituras concorrentes frequentes, menos contenção).
- `clients` é mapeado por `username` → `(SocketAddr, Tx)` para endereçar clientes por nome de usuário.
- Deduplicação: `seen_message_ids` evita reprocessar DMs com o mesmo `message_id` (útil quando cliente reenvia).

Canais e Entrega de Mensagens
-----------------------------
- O servidor usa canais Tokio (`tokio::sync::mpsc`) com capacidade limitada: **1000** mensagens por canal (buffer bounded).
- Para envios não-críticos (broadcasts, `RoomText`, mensagens de sistema), usamos `try_send`, que é síncrono e não bloqueia: se o buffer estiver cheio a mensagem é rejeitada (o servidor loga o evento e potencialmente descarta a mensagem).
- Para DMs (`Private`) e outros casos onde precisamos confirmar entrega ao remetente, o servidor usa um helper `send_reliable` que, quando há um runtime Tokio ativo, `spawn`s uma task que executa `tx.send(msg).await` (garantindo enfileiramento/entrega enquanto houver runtime). Quando não há runtime (ex.: em alguns testes unitários), há um fallback para `try_send`.
- Essa combinação mantém o servidor responsivo (sem bloquear em broadcasts) e permite garantias práticas de entrega para DMs; quando uma tentativa realmente falha o remetente recebe um `AckKind::Failed` ou `Error`, e o cliente é responsável por retry/expiração.

Comportamento quando o buffer encher
-----------------------------------
- Exemplo 1 — Broadcast (não-crítico): se o canal do cliente estiver cheio, `try_send` falha e o servidor loga e descarta a mensagem. O cliente não recebe nada; o servidor pode optar por métricas/alertas para monitorar esse evento.

  Exemplo JSON do log (ilustrativo):

```text
ERROR to=bob error=Full("channel") Falha ao enviar broadcast
```

- Exemplo 2 — DM crítica (com `message_id`): o servidor tenta entregar com `send().await` (rodando em task). Se mesmo assim a entrega falhar (cliente desconectado ou canal fechado), o remetente recebe um `AckKind::Failed` ou `Error` com o mesmo `message_id`, permitindo ao cliente correlacionar e parar retries.

  Exemplo DM-ACK quando o buffer/entrega falha:

```json
{"type":"Ack","kind":"Failed","info":"Falha ao entregar DM para bob.","message_id":"550e84..."}
```

Esses exemplos ajudam a entender trade-offs: broadcasts permanecem responsivos mesmo sob carga, enquanto DMs recebem esforço adicional para garantir entrega quando possível.


Protocolos e formato de mensagens
--------------------------------
- JSON por linha (`serde_json::to_string` + `\\n`).
- Timestamps: `chrono::DateTime<Utc>` serializado em ISO8601 UTC.
- `message_id`: UUIDv4 para correlação entre envio e ACK.

Testes e Qualidade
------------------
- `cargo test` roda testes unitários presentes no crate (ex.: fluxo DM+ACK, ChatState).
- Estratégias recomendadas:
  - Unit tests: lógica de `ChatCore` e `ChatState`.
  - Integração leve: executar servidor em background e simular múltiplos clientes TCP para fluxos end-to-end.

Testes de Integração End-to-End
-------------------------------
O projeto inclui uma suíte completa de testes E2E que validam o servidor com clientes TCP reais em [tests/integration_test.rs](tests/integration_test.rs).

**Testes implementados:**
- ✅ `test_login_and_broadcast` — Login de dois clientes + broadcast funcionando
- ✅ `test_dm_with_ack` — DM privada entre clientes + ACK Delivered
- ✅ `test_list_users` — Comando /list retornando usuários online
- ✅ `test_room_join_and_text` — Join em sala (#dev) + mensagens RoomText
- ✅ `test_dm_to_nonexistent_user` — Erro ao enviar DM para usuário inexistente
- ✅ `test_message_deduplication` — Deduplicação de message_id (não envia duplicatas)
- ✅ `test_rate_limiting` — Rate limit (10 msgs/s) detectado e aplicado
- ✅ `test_disconnect_cleanup` — Cleanup de usuário ao desconectar

**Resultado:** **8/8 testes passando** 🎉

**Como rodar:**
```bash
# Todos os testes E2E
cargo test --test integration_test -- --test-threads=1

# Teste específico
cargo test --test integration_test test_dm_with_ack -- --nocapture
```

**Nota:** Testes E2E usam `--test-threads=1` para evitar conflitos de porta (cada teste spawna um servidor em porta aleatória).

**Cobertura:**
- Fluxos de autenticação (Login)
- Broadcasts públicos (Text)
- DMs privadas com ACKs (Private + Ack Delivered)
- Comandos (/list, join/leave rooms)
- Deduplicação de mensagens
- Rate limiting (10 msgs/s)
- Cleanup de desconexão
- Tratamento de erros (usuário inexistente)

Stress tests (ignored)
----------------------
Há um teste de stress/concorrência mais pesado marcado como `#[ignore]` (simula joins/leave/broadcast concorrentes). Ele é intencionalmente ignorado em `cargo test` padrão para manter a execução rápida.

Para executar esse teste especificamente, use o target `stress-test` do `Makefile` ou rode diretamente:

```bash
make stress-test
# ou
cargo test -- --ignored --nocapture
```

Recomenda-se rodar o teste de stress em uma máquina de desenvolvimento local (não em CI por padrão); aumente `n`/`iterations` dentro do teste para intensidade maior.

Valores recomendados e como ajustar
----------------------------------
Para runs locais mais pesadas recomendamos começar com algo como:

- `n = 50` (50 usuários simultâneos)
- `iterations = 1000` (número de ciclos de join/leave)

Você pode ajustar esses parâmetros editando o teste em [src/server/state.rs](src/server/state.rs#L286). Procure pela função `stress_test_concurrent_joins_and_broadcast` e altere as variáveis `n` e `iterations` conforme desejado. Exemplo (linha aproximada):

```rust
// dentro de `stress_test_concurrent_joins_and_broadcast()`
let n = 50usize;
let iterations = 1000usize;
```

Nota: o teste está marcado com `#[ignore]` para não atrapalhar `cargo test` normal — execute-o explicitamente com `make stress-test`.
Recomenda-se rodar o teste de stress em uma máquina de desenvolvimento local (não em CI por padrão); aumente `n`/`iterations` dentro do teste para intensidade maior.

Logging e Observabilidade
-------------------------
- `tracing` + `tracing-subscriber` para logs estruturados.
- Controles:
  - `RUST_LOG` controla nível (`info`, `debug`, etc.). Ex.: `RUST_LOG=chat_serve=debug`.
  - `LOG_JSON=1` ativa formatação JSON (o `main.rs` inicializa condicionalmente o formatter JSON quando esta variável está presente).
  - `SHOW_ADDRS` (opcional) controla exibição de `SocketAddr` por privacidade (valores aceitos: `1`, `true`, `yes`).
- Recomendação: para produção envie logs JSON para um agregador (ELK, Loki) usando Filebeat/Promtail.

  Monitoramento rápido (logs)
  --------------------------
  Se você quiser detectar rapidamente eventos de buffer cheio / falha de envio, use comandos simples de logs:

  - Verificar contagem de eventos de falha (sistema via `journalctl`):

  ```bash
  journalctl -u chat_server -o cat | grep -c "Falha ao enviar"
  ```

  - Acompanhar logs em tempo real e filtrar por falhas de envio:

  ```bash
  journalctl -fu chat_server -o cat | grep --line-buffered "Falha ao enviar"
  ```

  - Se estiver rodando em Docker, use:

  ```bash
  docker logs -f chat_server_container 2>&1 | grep --line-buffered "Falha ao enviar"
  ```

  - Para procurar entradas específicas (ex.: falhas de DM):

  ```bash
  grep -R "Falha ao enviar DM" /var/log | wc -l
  ```

  Esses comandos são exemplos rápidos; em produção prefira enviar logs para um agregador e criar alertas/metricas para `Falha ao enviar` ou similares.

Métricas Prometheus e Healthcheck
--------------------------------
O servidor expõe endpoints HTTP para observabilidade:

- **`/metrics`** (porta 9090 por padrão) — métricas Prometheus incluindo:
  - `chat_send_full_total` — contador de eventos de canal cheio (`TrySendError::Full`)
  - `chat_active_sessions` — *gauge* com o número atual de sessões ativas
  - `chat_expired_sessions_total` — *counter* com total de sessões expiradas removidas pelo cleanup
  
- **`/health`** (porta 9090) — endpoint de healthcheck HTTP retornando JSON `{"status": "ok"}`
  - Útil para Docker HEALTHCHECK e orquestradores (Kubernetes readiness/liveness probes)

Configuração da porta do servidor de métricas:
  ```bash
  METRICS_BIND_ADDR=0.0.0.0:9090 cargo run --bin chat_server
  ```

Para consultar os endpoints:
  ```bash
  curl http://localhost:9090/metrics
  curl http://localhost:9090/health
  ```

Protegendo `/metrics`
---------------------
Se você quiser proteger o endpoint `/metrics`, defina a variável de ambiente `METRICS_BEARER_TOKEN` com um token secreto. Quando presente o servidor exigirá o header HTTP:

```
Authorization: Bearer <METRICS_BEARER_TOKEN>
```

Exemplo de uso com `curl`:

```bash
METRICS_BEARER_TOKEN=secret123 METRICS_BIND_ADDR=127.0.0.1:9090 cargo run --bin chat_server
# em outra janela:
curl -H "Authorization: Bearer secret123" http://127.0.0.1:9090/metrics
```

Tentativas não autorizadas
--------------------------
Todas as tentativas não autorizadas de acessar `/metrics` são logadas com nível `warn` e também incrementam a métrica Prometheus `chat_metrics_unauthorized_total`.
Use essa métrica para criar alertas quando houver acessos suspeitos ao endpoint de métricas.

Docker HEALTHCHECK (exemplo):
  ```dockerfile
  HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/chat_server", "--help"] || exit 1
  ```

Todos os `Dockerfile*` já incluem a diretiva HEALTHCHECK. O `docker-compose.yml` também contém configuração de healthcheck para o serviço `chat_server`.

Recomendação para produção:
- Monitore `chat_send_full_total` com alertas para detectar clientes lentos ou buffer overflow.
- Use `/health` para readiness/liveness probes no Kubernetes ou health checks em Docker Swarm/ECS.
- Consulte [monitoring/README.md](monitoring/README.md) para exemplos completos de Prometheus, Grafana e alertas.

Docker — imagens e fluxos
-------------------------
Implementamos múltiplas variantes para equilibrar reprodutibilidade, tamanho e usabilidade local:

1) Multi-stage (reprodutível, recomendado para CI/prod)
- `Dockerfile` (multi-stage builder Rust `rust:1.75-slim` → runtime `gcr.io/distroless/cc-debian12`).
- Resultado: imagem muito pequena (~20–50MB).
- Uso:
  ```bash
  docker build -t chat_server:latest .
  docker run --rm -p 8080:8080 -e RUST_LOG=info -e LOG_JSON=1 chat_server:latest
  ```

2) Debuggable runtime
- `Dockerfile.debian` (multi-stage builder → runtime `debian:bookworm-slim`).
- Permite `docker exec -it` e contém ferramentas básicas para debugging.
- Uso:
  ```bash
  make docker-build-debian
  make docker-run-debian
  ```

3) Fast-local (iterativo)
- `Dockerfile.local` (não compila, apenas copia `target/release/chat_server`).
- Muito rápido para testar alterações quando você compila localmente.
- Workflow:
  ```bash
  make docker-build-release   # roda `cargo build --release` e builda Dockerfile.local
  make docker-run-local
  ```
- Recomendação: use para desenvolvimento local; cuidado com diferenças de ambiente (host vs container).

4) Embutir certificados (opcional)
- `Dockerfile.with-certs` copia `certs/ca-certificates.crt` para dentro da imagem.
- Utilitário `make embed-certs` (ou `scripts/prepare-certs.sh`) copia bundle do host para `certs/` localmente.
- Fluxo:
  ```bash
  make embed-certs
  make docker-build-with-certs
  make docker-run-with-certs
  ```
- Nota: não comitar `certs/` (adicionado ao `.gitignore`).

Compose
- `docker-compose.yml` e `docker-compose.debian.yml` fornecem exemplos conceituais com `filebeat` para envio de logs.

Terminologia
-----------
Ao longo deste repositório usamos os termos "Private" e "DM" de forma intercambiável. Para consistência nas APIs e exemplos, prefira o termo **`Private` (Direct Message / DM)** nas mensagens JSON e documentação.

Variáveis de ambiente (resumo)
-----------------------------
As variáveis abaixo controlam comportamento de runtime e logs. Valores padrão são os usados nos exemplos do `README`.

| Variável | Descrição | Valor padrão |
|---|---:|---:|
| `RUST_LOG` | Nível de log/filtragem (`module=level`) | `info` |
| `LOG_JSON` | Se definido (qualquer valor), habilita logs em JSON | unset |
| `SHOW_ADDRS` | Mostrar `SocketAddr` nos logs (`1`/`true`) | unset |
| `PORT` | Porta TCP que o servidor escuta | `8080` |
| `BIND_ADDR` | Endereço de bind completo (`IP:PORT`) | `0.0.0.0:8080` |
| `RATE_LIMIT_PER_SEC` | Limite de mensagens/s por usuário | `10` |
| `RETRY_INTERVAL` | Intervalo de retry do cliente (segundos) | `3` |
| `RETRY_MAX_ATTEMPTS` | Tentativas máximas antes de marcar falha | `3` |

CI — exemplo GitHub Actions
---------------------------
Arquivo de exemplo: `.github/workflows/ci.yml` — roda testes Rust e constrói a imagem Docker (uploada artefato `chat_server.tar`).

Você pode adaptar o workflow para push/publish automatizado (requere segredos/permissions para registry).


Makefile — targets úteis
------------------------
Principais targets (resumo):
- `make build` — `cargo build`
- `make release` — `cargo build --release`
- `make run` — `cargo run --bin chat_server`
- `make client-run` — `cargo run --bin client`
- `make test` — `cargo test`
- `make fmt` — `cargo fmt --all`
- `make clippy` — `cargo clippy --all-targets -- -D warnings`

Docker-related:
- `make docker-build` — build padrão (multi-stage)
- `make docker-run` — run padrão
- `make docker-build-debian` / `make docker-run-debian`
- `make docker-build-release` — compila `cargo build --release` e usa `Dockerfile.local` (fast)
- `make docker-run-local`
- `make embed-certs` — copia bundle do host para `certs/` (local-only)
- `make docker-build-with-certs` / `make docker-run-with-certs`
- `make docker-compose-up` / `make docker-compose-debian-up`

Desenvolvimento local — fluxo recomendado
----------------------------------------
1. Iterar código e testes localmente:
   - `cargo test`
   - `cargo build` / `cargo run --bin chat_server`
2. Para iterar rapidamente com Docker:
   - `make docker-build-release` (fast: usa binário local)
   - `make docker-run-local` (montar host certs se necessário)
3. Para replicar ambiente de CI/produção use o `Dockerfile` multi-stage e `docker build` diretamente.

Graceful Shutdown
----------------
O servidor implementa graceful shutdown capturando sinais SIGINT (Ctrl+C) e SIGTERM:

- **Como funciona:**
  - O loop principal do servidor usa `tokio::select!` para monitorar simultaneamente novas conexões e sinais de shutdown
  - Quando SIGINT (Ctrl+C) ou SIGTERM é recebido, o servidor para de aceitar novas conexões
  - Conexões ativas em andamento continuam sendo processadas pelas tasks spawned até completarem
  - O servidor loga o evento de shutdown e encerra gracefully

- **Uso:**
  ```bash
  # Rodar servidor
  cargo run --bin chat_server
  
  # Pressionar Ctrl+C para shutdown graceful
  # Ou enviar SIGTERM (em outro terminal)
  pkill -TERM chat_server
  ```

- **Logs de shutdown:**
  ```text
  WARN 🛑 Sinal de shutdown recebido, encerrando servidor gracefully...
  INFO 👋 Servidor encerrado
  ```

- **Em Docker:** O Docker envia SIGTERM quando você executa `docker stop`. Com o graceful shutdown, o container aguarda até 10 segundos (padrão) para finalizar antes de forçar (SIGKILL).

- **Recomendações para produção:**
  - Em orquestradores (Kubernetes, ECS), configure `terminationGracePeriodSeconds` adequadamente para permitir que conexões longas finalizem
  - Use combinado com health checks (`/health`) para remover o servidor de load balancers antes de iniciar shutdown

Comandos úteis (exemplos)
- Rodar servidor local sem Docker:
  ```bash
  RUST_LOG=info cargo run --bin chat_server
  ```
- Rodar cliente:
  ```bash
  cargo run --bin client
  ```
- Rodar cliente com debug logging (útil para troubleshooting):
  ```bash
  RUST_LOG=debug cargo run --bin client
  ```
- Testes:
  ```bash
  cargo test --all
  ```
- Build e rodar imagem mínima:
  ```bash
  docker build -t chat_server:latest .
  docker run --rm -p 8080:8080 -e RUST_LOG=info -e LOG_JSON=1 chat_server:latest
  ```

Debug e Logging do Cliente
--------------------------
O cliente implementa logging detalhado usando `tracing` para facilitar debug:

- **Níveis de log disponíveis:**
  - `error` — Erros críticos (falhas de conexão, serialização, I/O)
  - `warn` — Avisos (retries, conexão perdida)
  - `info` — Informações gerais (conexão estabelecida, export)
  - `debug` — Debug detalhado (mensagens enviadas/recebidas, IDs)

- **Ativando logs:**
  ```bash
  # Logs normais (info+)
  RUST_LOG=info cargo run --bin client
  
  # Debug completo
  RUST_LOG=debug cargo run --bin client
  
  # Debug apenas do cliente
  RUST_LOG=client=debug cargo run --bin client
  ```

- **Tipos de erros logados:**
  - Falhas de conexão TCP com o servidor
  - Erros de serialização JSON (envio)
  - Erros de deserialização JSON (recebimento do servidor)
  - Falhas de I/O na rede (envio/recebimento)
  - Tentativas de retry e falhas definitivas de DMs
  - Erros ao exportar histórico (`/export`)

- **Exemplo de saída com debug:**
  ```text
  DEBUG Tentando conectar a 127.0.0.1:8080
  INFO ✅ Conectado ao servidor em 127.0.0.1:8080
  DEBUG Enviando login como: alice
  INFO ✅ Login enviado para o servidor
  DEBUG to="bob" message_id="550e8400..." Enviando DM
  WARN 🔄 Reenviando (2/3) DM para bob: Olá
  ERROR ❌ Falha definitiva ao entregar DM após 3 tentativas
  ```

- **Logs do servidor (handle.rs):**
  - Logs estruturados com peer (IP ou username)
  - Erros de JSON inválido com raw line
  - Falhas de I/O ao enviar mensagens para clientes
  - Tipo de mensagem (discriminant) em debug

Segurança e TLS
---------------
- Por padrão, o runtime mínimo pode não incluir CA bundles. Para desenvolvimento sugerimos montar o CA bundle do host:
  ```bash
  docker run -v /etc/ssl/certs/ca-certificates.crt:/etc/ssl/certs/ca-certificates.crt:ro ...
  ```
- Alternativamente, use `make embed-certs` (local-only) e `Dockerfile.with-certs` para embutir a cadeia de confiança.
- NÃO comite `certs/` em repositórios públicos — `.gitignore` já ignora o diretório.

CI / Deploy (sugestões)
-----------------------
- CI pipeline mínimo:
  1. `cargo test --all`
  2. `cargo build --release`
  3. `docker build` usando `Dockerfile` multi-stage
  4. push para registry (ex.: ghcr, ECR)
- Para observabilidade, envie logs JSON para ELK/Loki e crie dashboards por `username` e `message_id`.

Arquivos importantes
-------------------
- `Cargo.toml` — dependências e versão
- `src/main.rs` — inicialização do servidor e tracing
- `src/server/core.rs` — lógica principal do servidor
- `src/server/state.rs` — `ChatState` e estruturas concorrentes
- `src/bin/client.rs` — cliente interativo
- `Dockerfile`, `Dockerfile.debian`, `Dockerfile.local`, `Dockerfile.with-certs`
- `Makefile` — comandos úteis
- `docker-compose.yml`, `docker-compose.debian.yml`
- `scripts/prepare-certs.sh`

Próximos passos sugeridos
-------------------------
- Implementar persistência opcional (`sled`, `sqlite`) para mensagens/ACKs para tolerância a falhas.
- Adicionar workflows GitHub Actions para build/test/push de imagens.
- Adicionar testes de integração end-to-end que executem o servidor em um container e usem clientes automatizados.

Prometheus — instalar regra de alerta
------------------------------------
Para ativar a regra de alerta fornecida em [monitoring/alert_rules.yml](monitoring/alert_rules.yml):

1. Copie `monitoring/alert_rules.yml` para o diretório onde seu Prometheus busca regras, ou referencie-o diretamente no `prometheus.yml`.

2. Exemplo mínimo de `prometheus.yml` (adicione o `rule_files` e um `scrape_config` para o serviço de métricas):

```yaml
global:
  scrape_interval: 15s

rule_files:
  - 'monitoring/alert_rules.yml'  # ajuste path conforme sua instalação

scrape_configs:
  - job_name: 'chat_serve'
    static_configs:
      - targets: ['chat_server:9090'] # em Docker Compose, ajuste para o nome do serviço

scrape_configs:
  - job_name: 'chat_serve'
    static_configs:
      - targets: ['localhost:9090']
```

3. Recarga das regras (sem reiniciar Prometheus):

```bash
# API HTTP de reload (Prometheus precisa permitir reload via endpoint)
curl -X POST http://localhost:9090/-/reload

# ou, se estiver como systemd service
systemctl reload prometheus
```

4. Verifique se a regra foi carregada e/ou se o alerta dispara consultando o UI do Prometheus em `/alerts` ou usando o `promtool`:

```bash
promtool check rules monitoring/alert_rules.yml
```

Observação: ajuste `targets` e caminhos conforme a topologia do seu ambiente (Docker, Kubernetes, host). Em Kubernetes é comum usar `ServiceMonitor`/`PodMonitor` via Prometheus Operator.

Exemplos de implantação
-----------------------

Docker Compose (exemplo mínimo):

```yaml
version: '3.7'
services:
  chat_server:
    image: chat_server:latest
    ports:
      - "8080:8080"
      - "9090:9090"
    environment:
      - METRICS_BIND_ADDR=0.0.0.0:9090

  prometheus:
    image: prom/prometheus:latest
    volumes:
      - ./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - ./monitoring/alert_rules.yml:/etc/prometheus/alert_rules.yml:ro
    ports:
      - "9091:9090"

```

Kubernetes (Prometheus Operator) — `Service` + `ServiceMonitor` (exemplo):

```yaml
# Service para expor métricas do chat_server
apiVersion: v1
kind: Service
metadata:
  name: chat-server-metrics
  labels:
    app: chat-server
spec:
  ports:
    - name: metrics
      port: 9090
      targetPort: 9090
  selector:
    app: chat-server

---
# ServiceMonitor (Prometheus Operator) para scrappear /metrics
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: chat-server-monitor
  labels:
    release: prometheus
spec:
  selector:
    matchLabels:
      app: chat-server
  endpoints:
    - port: metrics
      path: /metrics
      interval: 15s
```

Contato / Contribuição
----------------------
- Pull requests são bem-vindos. Abra PRs pequenos focados em uma única mudança.
- Para features maiores (persistência, autenticação), abra uma issue antes de implementar.



