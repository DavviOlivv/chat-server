# 👮 Comandos de Administração - Guia Completo

Sistema completo de moderação e administração para o chat-serve.

## 📋 Visão Geral

O sistema de administração permite:
- **Moderação básica**: Expulsar (kick), banir, silenciar usuários
- **Gestão de permissões**: Promover/remover administradores
- **Auditoria**: Logs completos de todas as ações de moderação
- **Persistência**: Todos os dados salvos no SQLite

## 🎖️ Hierarquia de Permissões

```
👤 Usuário Normal
   ├─ Enviar mensagens
   ├─ Entrar em salas
   └─ Ver usuários online

👮 Administrador
   ├─ Todas as permissões de usuário
   ├─ /kick - Expulsar usuários
   ├─ /ban - Banir temporária ou permanentemente
   ├─ /mute - Silenciar usuários
   ├─ /unmute - Remover silenciamento
   ├─ /promote - Promover a admin
   ├─ /demote - Remover admin
   ├─ /adminlist - Listar admins
   └─ /adminlogs - Ver logs de moderação
```

## 🔧 Comandos Disponíveis

### 1. `/kick <usuário> <motivo>` - Expulsar

**Função**: Desconecta usuário imediatamente do servidor.

**Uso**:
```bash
/kick bob Comportamento inadequado
/kick alice Spam excessivo
```

**Efeito**:
- ✅ Usuário é desconectado imediatamente
- ✅ Pode reconectar (não é ban)
- ✅ Log de moderação criado
- ✅ Usuário recebe notificação com motivo

**Resposta**:
```
Admin: ✅ bob foi expulso: Comportamento inadequado
Bob: Você foi expulso por alice: Comportamento inadequado
```

---

### 2. `/ban <usuário> <duração> <motivo>` - Banir

**Função**: Impede login do usuário por tempo determinado.

**Uso**:
```bash
/ban bob 3600 Spam (1 hora)
/ban charlie 86400 Ofensas (24 horas)
/ban mallory 0 Violação grave (permanente)
```

**Duração**:
- `N > 0` - Banimento por N segundos
- `0` - Banimento permanente

**Efeito**:
- ✅ Usuário desconectado imediatamente
- ✅ Não pode fazer login até expiração
- ✅ Login retorna erro: "Você está banido deste servidor"
- ✅ Banimento expira automaticamente

**Conversões de Tempo**:
```
1 hora    = 3600
6 horas   = 21600
12 horas  = 43200
24 horas  = 86400
7 dias    = 604800
30 dias   = 2592000
```

**Resposta**:
```
Admin: ✅ bob foi banido por 3600 segundos: Spam
```

---

### 3. `/mute <usuário> <duração>` - Silenciar

**Função**: Impede usuário de enviar mensagens por tempo determinado.

**Uso**:
```bash
/mute bob 600 (10 minutos)
/mute alice 3600 (1 hora)
```

**Efeito**:
- ✅ Usuário pode ver mensagens
- ❌ Não pode enviar mensagens
- ✅ Comandos admin ainda funcionam (se for admin)
- ✅ Expira automaticamente

**Quando usuário tenta falar**:
```
🔇 Você está silenciado e não pode enviar mensagens
```

**Resposta**:
```
Admin: ✅ bob foi silenciado por 600 segundos
Bob: Você foi silenciado por 600 segundos
```

---

### 4. `/unmute <usuário>` - Remover Silêncio

**Função**: Remove silenciamento antes da expiração.

**Uso**:
```bash
/unmute bob
```

**Efeito**:
- ✅ Usuário pode enviar mensagens novamente
- ✅ Notificação enviada ao usuário

**Resposta**:
```
Admin: ✅ bob foi desilenciado
Bob: Você foi desilenciado
```

---

### 5. `/promote <usuário>` - Promover a Admin

**Função**: Concede permissões de administrador.

**Uso**:
```bash
/promote bob
```

**Efeito**:
- ✅ Usuário ganha todos os comandos admin
- ✅ Log de moderação criado
- ✅ Notificação enviada

**Resposta**:
```
Admin: ✅ bob foi promovido a admin
Bob: 🎉 Você foi promovido a admin!
```

---

### 6. `/demote <usuário>` - Remover Admin

**Função**: Remove permissões de administrador.

**Uso**:
```bash
/demote bob
```

**Efeito**:
- ❌ Usuário perde comandos admin
- ✅ Mantém acesso normal
- ✅ Log de moderação criado

**Resposta**:
```
Admin: ✅ bob perdeu status de admin
Bob: Você perdeu status de admin
```

---

### 7. `/adminlist` - Listar Admins

**Função**: Mostra todos os administradores atuais.

**Uso**:
```bash
/adminlist
```

**Resposta**:
```
👮 Admins (3): alice, bob, charlie
```

Ou se nenhum:
```
Nenhum admin cadastrado
```

---

### 8. `/adminlogs [limite]` - Ver Logs

**Função**: Mostra logs de moderação recentes.

**Uso**:
```bash
/adminlogs           (últimos 20)
/adminlogs 50        (últimos 50)
/adminlogs 100       (últimos 100)
```

**Resposta**:
```
📋 Logs de Moderação (últimos 5):
[2026-03-05T14:30:00Z] alice → bob (kick): Spam excessivo
[2026-03-05T14:25:00Z] alice → charlie (ban): Violação de regras - duração: 3600s
[2026-03-05T14:20:00Z] alice → mallory (mute): - duração: 600s
[2026-03-05T14:15:00Z] alice → bob (promote): 
[2026-03-05T14:10:00Z] alice → eve (unmute): 
```

---

## 🗄️ Estrutura do Banco de Dados

### Tabela `admins`
```sql
CREATE TABLE admins (
    username TEXT PRIMARY KEY,
    promoted_at TEXT NOT NULL,
    promoted_by TEXT NOT NULL
);
```

### Tabela `bans`
```sql
CREATE TABLE bans (
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL,
    banned_by TEXT NOT NULL,
    reason TEXT NOT NULL,
    banned_at TEXT NOT NULL,
    expires_at TEXT,           -- NULL = permanente
    active BOOLEAN DEFAULT 1
);
```

### Tabela `mutes`
```sql
CREATE TABLE mutes (
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL,
    muted_by TEXT NOT NULL,
    muted_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    active BOOLEAN DEFAULT 1
);
```

### Tabela `moderation_logs`
```sql
CREATE TABLE moderation_logs (
    id INTEGER PRIMARY KEY,
    action TEXT NOT NULL,
    moderator TEXT NOT NULL,
    target_user TEXT NOT NULL,
    reason TEXT,
    details TEXT,
    timestamp TEXT NOT NULL
);
```

---

## 🔐 Segurança e Validações

### Verificações Automáticas

**No Login**:
```rust
if db.is_banned(username) {
    return Err("Você está banido deste servidor");
}
```

**Ao Enviar Mensagens**:
```rust
if db.is_muted(username) {
    return Error("Você está silenciado");
}
```

**Nos Comandos Admin**:
```rust
if !db.is_admin(username) {
    return Error("Sem permissão (requer admin)");
}
```

### Proteções

✅ **Não pode promover usuário inexistente**
✅ **Não pode banir usuário inexistente**
✅ **Ban/mute expiram automaticamente**
✅ **Logs são imutáveis (append-only)**
✅ **Admin não pode perder última permissão** (recomendado implementar)

---

## 📊 Fluxos de Uso

### Cenário 1: Moderação de Spam

```bash
# 1. Admin detecta spam
Admin: "bob está fazendo spam!"

# 2. Silencia por 10 minutos
admin> /mute bob 600

# Resposta
✅ bob foi silenciado por 600 segundos

# 3. Bob tenta enviar mensagem
bob> Olá!
🔇 Você está silenciado e não pode enviar mensagens

# 4. Após reflexão, admin remove mute
admin> /unmute bob

# Resposta
✅ bob foi desilenciado
```

### Cenário 2: Banimento Temporário

```bash
# 1. Usuário quebra regras graves
admin> /ban charlie 86400 Ofensas repetidas

# Resposta
✅ charlie foi banido por 86400 segundos: Ofensas repetidas

# 2. Charlie tenta reconectar
username: charlie
password: ••••••
❌ Você está banido deste servidor

# 3. Após 24h, banimento expira automaticamente
# Charlie pode reconectar
```

### Cenário 3: Gestão de Admins

```bash
# 1. Promover moderador confiável
alice> /promote bob

# Resposta
✅ bob foi promovido a admin
[Bob recebe] 🎉 Você foi promovido a admin!

# 2. Bob agora pode usar comandos
bob> /adminlist
👮 Admins (2): alice, bob

# 3. Se bob abusar, remover permissões
alice> /demote bob
✅ bob perdeu status de admin
```

### Cenário 4: Auditoria

```bash
# Ver últimas ações
admin> /adminlogs 10

📋 Logs de Moderação (últimos 10):
[2026-03-05T15:30:00Z] alice → bob (promote): 
[2026-03-05T15:25:00Z] alice → charlie (ban): Spam - duração: 3600s
[2026-03-05T15:20:00Z] bob → mallory (kick): Linguagem ofensiva
[2026-03-05T15:15:00Z] alice → eve (mute): - duração: 600s
[2026-03-05T15:10:00Z] bob → dave (unmute): 
...
```

---

## 🛠️ Uso nos Clientes

### Cliente CLI Original

```bash
# Autenticar
{"type":"Login","username":"alice","password":"senha123"}

# Executar comando admin
{
  "type":"Authenticated",
  "token":"<token>",
  "action":{
    "AdminKick":{
      "target":"bob",
      "reason":"Spam"
    }
  }
}
```

### Cliente TUI

Adicionar comandos no input:
```
alice> /kick bob Spam
alice> /ban charlie 3600 Ofensas
alice> /mute mallory 600
alice> /adminlist
alice> /adminlogs 20
```

### Cliente Web

```javascript
// Kick
const kickCommand = {
    type: 'Authenticated',
    token: sessionToken,
    action: {
        action_type: 'AdminKick',
        target: 'bob',
        reason: 'Spam excessivo'
    }
};
ws.send(JSON.stringify(kickCommand));

// Ban por 1 hora
const banCommand = {
    type: 'Authenticated',
    token: sessionToken,
    action: {
        action_type: 'AdminBan',
        target: 'charlie',
        duration_secs: 3600,
        reason: 'Violação de regras'
    }
};
ws.send(JSON.stringify(banCommand));
```

---

## 📈 Métricas e Monitoramento

### Logs de Servidor

```
INFO  chat_serve::server::core > 🔨 Admin KICK admin=alice target=bob reason=Spam
INFO  chat_serve::server::core > 🚫 Admin BAN admin=alice target=charlie duration=3600 reason=Ofensas
INFO  chat_serve::server::core > 🔇 Admin MUTE admin=alice target=mallory duration=600
INFO  chat_serve::server::core > ⬆️ Admin PROMOTE admin=alice target=bob
```

### Queries Úteis

```sql
-- Banimentos ativos
SELECT username, banned_by, reason, expires_at 
FROM bans 
WHERE active = 1 
ORDER BY banned_at DESC;

-- Silenciamentos ativos
SELECT username, muted_by, expires_at 
FROM mutes 
WHERE active = 1 
AND expires_at > datetime('now')
ORDER BY muted_at DESC;

-- Ações por moderador
SELECT action, COUNT(*) 
FROM moderation_logs 
GROUP BY moderator, action 
ORDER BY COUNT(*) DESC;

-- Usuários mais moderados
SELECT target_user, COUNT(*) 
FROM moderation_logs 
GROUP BY target_user 
ORDER BY COUNT(*) DESC 
LIMIT 10;
```

---

## ⚠️ Boas Práticas

### Para Administradores

1. **Sempre forneça motivo claro** nos banimentos
2. **Use temporários primeiro** - ban permanente é extremo
3. **Documente casos graves** - use /adminlogs regularmente
4. **Avise antes de mute** - dê chance de correção
5. **Revise banimentos** - pessoas mudam

### Para Desenvolvedores

1. **Backups do banco** - logs são evidência
2. **Rate limiting** - previne abuso de comandos
3. **Admin inicial** - crie via SQL diretamente:
   ```sql
   INSERT INTO admins VALUES ('admin', datetime('now'), 'system');
   ```
4. **Monitoramento** - alerte em ações em massa
5. **Auditoria externa** - logs devem ser append-only

---

## 🚀 Criando Primeiro Admin

Como não há admins inicialmente, crie o primeiro diretamente no banco:

```bash
# Conectar ao banco
sqlite3 chat.db

# Inserir admin inicial
INSERT INTO admins (username, promoted_at, promoted_by) 
VALUES ('alice', datetime('now'), 'system');

# Verificar
SELECT * FROM admins;
```

Ou via código no servidor:

```rust
// No startup do servidor
if let Ok(admins) = db.list_admins() {
    if admins.is_empty() {
        db.promote_admin("alice", "system")?;
        println!("✅ Admin inicial 'alice' criado");
    }
}
```

---

## 🐛 Troubleshooting

### "Banco de dados não disponível"
- Servidor iniciado sem `--database` flag
- Verificar: `cargo run --bin chat_server -- --database chat.db`

### "Sem permissão (requer admin)"
- Usuário não está na tabela `admins`
- Verificar: `SELECT * FROM admins WHERE username = 'alice';`
- Criar: `/promote alice` (como outro admin) ou SQL direto

### Ban não expira
- Verificar timestamp: `SELECT expires_at FROM bans WHERE username = 'bob';`
- Formato deve ser RFC3339: `2026-03-05T15:30:00Z`
- Remover manualmente: `UPDATE bans SET active = 0 WHERE username = 'bob';`

### Mute não funciona
- Verificar expiração: `SELECT * FROM mutes WHERE username = 'bob' AND active = 1;`
- Mute expira se `expires_at < NOW()`
- Desativar: `/unmute bob` ou `UPDATE mutes SET active = 0`

---

## 📚 Referências

- **Código**: [src/model/message.rs](../src/model/message.rs) - Definição de ChatAction
- **Database**: [src/server/database.rs](../src/server/database.rs) - Operações de moderação
- **Core**: [src/server/core.rs](../src/server/core.rs) - Lógica de comandos
- **Auth**: [src/server/auth.rs](../src/server/auth.rs) - Verificação de ban no login

---

## ✅ Conclusão

Sistema de administração completo com:
- 8 comandos de moderação
- Persistência no SQLite
- Logs completos de auditoria
- Expiração automática
- Verificações de segurança

**Pronto para uso em produção!** 🎉
