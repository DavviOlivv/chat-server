# Chat Server Protocol Specification

Este documento define o protocolo de comunicação do chat-serve, incluindo schemas JSON, fluxos de mensagens e exemplos.

## 📡 Visão Geral

- **Transport:** WebSocket TCP (porta configurável, padrão 8080)
- **Encoding:** JSON UTF-8
- **Line-delimited:** Cada mensagem é um objeto JSON em uma única linha terminada por `\n`
- **Bidirecional:** Cliente e servidor trocam mensagens `ChatMessage`

## 📦 Estrutura Base: ChatMessage

Todas as mensagens seguem o enum `ChatMessage` serializado em JSON:

```typescript
type ChatMessage = 
  | RegisterRequest
  | LoginRequest
  | LogoutRequest
  | JoinRoomRequest
  | LeaveRoomRequest
  | ListUsersRequest
  | RoomTextRequest
  | PrivateMessageRequest
  | Broadcast
  | PrivateMessage
  | Ack;
```

## 🔐 Autenticação

### 1. Register (Cliente → Servidor)

**Request:**
```json
{
  "Register": {
    "username": "alice",
    "password": "secret123"
  }
}
```

**Response (Success):**
```json
{
  "Ack": {
    "kind": "System",
    "info": "Conta criada: alice",
    "message_id": null
  }
}
```

**Response (Error):**
```json
{
  "Ack": {
    "kind": "Failed",
    "info": "Usuário já existe",
    "message_id": null
  }
}
```

### 2. Login (Cliente → Servidor)

**Request:**
```json
{
  "Login": {
    "username": "alice",
    "password": "secret123"
  }
}
```

**Response (Success):**
```json
{
  "Ack": {
    "kind": "System",
    "info": "Login realizado: alice",
    "message_id": null
  }
}
```

Seguido por mensagens offline pendentes (se houver):

```json
{
  "Ack": {
    "kind": "System",
    "info": "📬 Você tem 3 mensagens novas recebidas enquanto estava offline:",
    "message_id": null
  }
}
```

```json
{
  "PrivateMessage": {
    "from": "bob",
    "to": "alice",
    "content": "Oi Alice!",
    "timestamp": "2026-03-04T10:30:00Z",
    "message_id": "msg_123"
  }
}
```

**Response (Error):**
```json
{
  "Ack": {
    "kind": "Failed",
    "info": "Credenciais inválidas",
    "message_id": null
  }
}
```

### 3. Logout (Cliente → Servidor)

**Request:**
```json
{
  "Logout": {}
}
```

**Response:**
```json
{
  "Ack": {
    "kind": "System",
    "info": "Logout realizado",
    "message_id": null
  }
}
```

## 🏠 Salas (Rooms)

### 4. Join Room (Cliente → Servidor)

**Request:**
```json
{
  "JoinRoom": {
    "room": "general"
  }
}
```

**Response (Success):**
```json
{
  "Ack": {
    "kind": "System",
    "info": "Entrou na sala: general",
    "message_id": null
  }
}
```

**Broadcast para todos na sala:**
```json
{
  "Broadcast": {
    "room": "general",
    "from": "alice",
    "content": "alice entrou na sala",
    "timestamp": "2026-03-05T14:20:00Z",
    "message_id": "msg_456"
  }
}
```

### 5. Leave Room (Cliente → Servidor)

**Request:**
```json
{
  "LeaveRoom": {
    "room": "general"
  }
}
```

**Response:**
```json
{
  "Ack": {
    "kind": "System",
    "info": "Saiu da sala: general",
    "message_id": null
  }
}
```

**Broadcast para todos na sala:**
```json
{
  "Broadcast": {
    "room": "general",
    "from": "alice",
    "content": "alice saiu da sala",
    "timestamp": "2026-03-05T14:25:00Z",
    "message_id": "msg_457"
  }
}
```

### 6. Room Message (Cliente → Servidor)

**Request:**
```json
{
  "RoomText": {
    "room": "general",
    "content": "Olá a todos!",
    "message_id": "client_msg_001"
  }
}
```

**Response (ACK ao remetente):**
```json
{
  "Ack": {
    "kind": "Delivered",
    "info": "Mensagem entregue na sala general",
    "message_id": "client_msg_001"
  }
}
```

**Broadcast para todos na sala (exceto remetente):**
```json
{
  "Broadcast": {
    "room": "general",
    "from": "alice",
    "content": "Olá a todos!",
    "timestamp": "2026-03-05T14:30:00Z",
    "message_id": "msg_458"
  }
}
```

## 💬 Mensagens Privadas

### 7. Private Message (Cliente → Servidor)

**Request:**
```json
{
  "PrivateMessage": {
    "from": "alice",
    "to": "bob",
    "content": "Oi Bob, tudo bem?",
    "timestamp": "2026-03-05T14:35:00Z",
    "message_id": "client_msg_002"
  }
}
```

**Caso 1: Destinatário online**

Response ao remetente:
```json
{
  "Ack": {
    "kind": "Delivered",
    "info": "DM entregue para bob",
    "message_id": "client_msg_002"
  }
}
```

Entrega ao destinatário:
```json
{
  "PrivateMessage": {
    "from": "alice",
    "to": "bob",
    "content": "Oi Bob, tudo bem?",
    "timestamp": "2026-03-05T14:35:00Z",
    "message_id": "msg_459"
  }
}
```

**Caso 2: Destinatário offline**

Response ao remetente:
```json
{
  "Ack": {
    "kind": "Received",
    "info": "bob está offline. Mensagem será entregue quando voltar online",
    "message_id": "client_msg_002"
  }
}
```

**Caso 3: Usuário não existe**

Response ao remetente:
```json
{
  "Ack": {
    "kind": "Failed",
    "info": "Usuário bob não existe",
    "message_id": "client_msg_002"
  }
}
```

**Caso 4: Caixa de entrada cheia**

Response ao remetente:
```json
{
  "Ack": {
    "kind": "Failed",
    "info": "Caixa de entrada de bob está cheia. Tente mais tarde",
    "message_id": "client_msg_002"
  }
}
```

## 📋 Listar Usuários

### 8. List Users (Cliente → Servidor)

**Request:**
```json
{
  "ListUsers": {}
}
```

**Response:**
```json
{
  "Ack": {
    "kind": "System",
    "info": "Usuários online: alice, bob, charlie",
    "message_id": null
  }
}
```

## 🔄 Fluxo de Handshake Completo

```
Cliente                                    Servidor
   |                                          |
   |------- TCP Connect -------------------->|
   |                                          |
   |------- Register/Login ------------------>|
   |<------ Ack (System) ---------------------|
   |                                          |
   | (se Login com mensagens pendentes)       |
   |<------ Ack (System: "X mensagens...") ---|
   |<------ PrivateMessage (msg 1) ----------|
   |<------ PrivateMessage (msg 2) ----------|
   |<------ PrivateMessage (msg N) ----------|
   |                                          |
   |------- JoinRoom("general") ------------->|
   |<------ Ack (System) ---------------------|
   |<------ Broadcast ("alice entrou") -------|
   |                                          |
   |------- RoomText("Olá!") ---------------->|
   |<------ Ack (Delivered) ------------------|
   |                                          |
   | (outros clientes recebem)                |
   |        <------ Broadcast ("Olá!") -------|
   |                                          |
   |------- PrivateMessage(to: bob) --------->|
   |<------ Ack (Delivered/Received) ---------|
   |                                          |
   |------- Logout -------------------------->|
   |<------ Ack (System) ---------------------|
   |                                          |
   |------- TCP Close ----------------------->|
```

## 📊 JSON Schemas

### AckKind Enum
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "string",
  "enum": ["Delivered", "Received", "Read", "Failed", "System"]
}
```

### Ack Schema
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "Ack": {
      "type": "object",
      "properties": {
        "kind": { "$ref": "#/definitions/AckKind" },
        "info": { "type": "string" },
        "message_id": { "type": ["string", "null"] }
      },
      "required": ["kind", "info", "message_id"]
    }
  },
  "required": ["Ack"]
}
```

### PrivateMessage Schema
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "PrivateMessage": {
      "type": "object",
      "properties": {
        "from": { "type": "string", "minLength": 3, "maxLength": 32 },
        "to": { "type": "string", "minLength": 3, "maxLength": 32 },
        "content": { "type": "string", "minLength": 1 },
        "timestamp": { "type": "string", "format": "date-time" },
        "message_id": { "type": "string" }
      },
      "required": ["from", "to", "content", "timestamp", "message_id"]
    }
  },
  "required": ["PrivateMessage"]
}
```

### Broadcast Schema
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "Broadcast": {
      "type": "object",
      "properties": {
        "room": { "type": "string", "minLength": 1, "maxLength": 64 },
        "from": { "type": "string" },
        "content": { "type": "string", "minLength": 1 },
        "timestamp": { "type": "string", "format": "date-time" },
        "message_id": { "type": "string" }
      },
      "required": ["room", "from", "content", "timestamp", "message_id"]
    }
  },
  "required": ["Broadcast"]
}
```

## ⚙️ Configuração

### Variáveis de Ambiente

| Variável | Tipo | Padrão | Descrição |
|----------|------|--------|-----------|
| `ADDR` | String | `127.0.0.1:8080` | Endereço do servidor |
| `DB_PATH` | String | `users.db` | Caminho do banco SQLite |
| `JWT_SECRET` | String | `secret` | Chave para tokens JWT |
| `JWT_EXPIRATION` | Number | `86400` | Expiração JWT (segundos) |
| `MAX_PENDING_PER_USER` | Number | `1000` | Limite de mensagens offline |
| `MESSAGE_RETENTION_DAYS` | Number | `90` | Retenção geral de mensagens |
| `PENDING_MESSAGE_TTL_DAYS` | Number | - | TTL de mensagens pendentes |

### Rate Limiting

- **Por usuário:** 10 mensagens/segundo
- **Janela:** 1 segundo deslizante
- **Ação:** Descarta mensagens excedentes (não envia ACK de erro)

## 🔒 Validações

### Username
- Comprimento: 3-32 caracteres
- Permitidos: alfanuméricos, underscore, hífen
- Regex: `^[a-zA-Z0-9_-]{3,32}$`

### Password
- Comprimento mínimo: 6 caracteres
- Armazenamento: bcrypt hash

### Room Name
- Comprimento: 1-64 caracteres
- Sem restrições especiais

### Message Content
- Comprimento mínimo: 1 caractere
- Sem limite superior explícito (limitado por memória)

## 📝 Notas de Implementação

1. **Deduplicação:** Servidor usa `message_id` para evitar processamento duplicado
2. **Timestamps:** Sempre em UTC ISO 8601
3. **Offline Messages:** Entregues em ordem cronológica no login
4. **Persistência:** Apenas mensagens privadas são persistidas no SQLite
5. **Concorrência:** Tokio async runtime, um task por conexão
6. **Graceful Shutdown:** SIGTERM/SIGINT fecham conexões antes de encerrar

## 🚀 Exemplos de Uso

### Curl (não recomendado, apenas ilustrativo)
WebSocket requer cliente específico. Use `websocat` ou similar:

```bash
# Instalar websocat
brew install websocat

# Conectar
websocat ws://127.0.0.1:8080

# Enviar (linha única)
{"Login":{"username":"alice","password":"secret123"}}
```

### Cliente incluído
```bash
cargo run --bin client -- --addr 127.0.0.1:8080
```

## 📚 Referências

- **Rust Structs:** Ver `src/model/message.rs`
- **Serialização:** `serde_json`
- **WebSocket:** `tokio_tungstenite`
- **Database Schema:** Ver `src/server/database.rs`
