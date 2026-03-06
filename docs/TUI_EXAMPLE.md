# Exemplo de Uso do Cliente TUI

Este guia mostra como usar todos os recursos do cliente TUI.

## 1. Instalação e Execução

```bash
# Compilar
cargo build --bin tui

# Executar
cargo run --bin tui

# Ou executar o binário diretamente
./target/debug/tui
```

## 2. Login

```
Conectando ao servidor...
Digite seu username: alice
Digite sua senha: ******
```

## 3. Interface Principal

Após o login, você verá:

```
┌──────────────────────────────────────────────────┐
│  [Geral] DMs Salas                              │
├────────────────────────────┬─────────────────────┤
│ Mensagens - 📍 Auto-scroll │ Online (3)          │
│                            │ • alice             │
│ [sistema] Bem-vindo!       │ • bob               │
│                            │ • charlie           │
└────────────────────────────┴─────────────────────┘
│ Input (Ctrl+C para sair)                        │
│ _                                               │
└──────────────────────────────────────────────────┘
```

## 4. Enviando Mensagens

### Broadcast (Mensagem Pública)

```
Digite: Olá pessoal!
Pressione: Enter
```

Resultado na aba **Geral**:
```
[14:30] alice: Olá pessoal!
```

### Mensagem Privada (DM)

```
Digite: /msg bob Quer conversar em particular?
Pressione: Enter
```

Resultado na aba **DMs**:
```
🤫 DM [14:31] bob: Quer conversar em particular?
```

Bob também verá na aba DMs dele:
```
🤫 DM [14:31] alice: Quer conversar em particular?
```

## 5. Entrando em Salas

```
Digite: /join dev
Pressione: Enter
```

Notificação aparece:
```
┌──────────────────────────────────────────────────┐
│ ⚠️ Aviso: Entrou na sala dev                    │
└──────────────────────────────────────────────────┘
```

Agora mensagens normais vão para a sala:
```
Digite: Alguém pode revisar meu PR?
Pressione: Enter
```

Resultado na aba **Salas**:
```
#dev [14:32] alice: Alguém pode revisar meu PR?
```

## 6. Navegação entre Abas

### Trocar para DMs
```
Pressione: Tab (ou Tab até chegar em DMs)
```

Se houver mensagens não lidas:
```
│  Geral [DMs (3)] Salas                          │
         ^^^^^^
```

Ao entrar na aba, o contador zera automaticamente.

### Voltar para Geral
```
Pressione: Shift+Tab
```

## 7. Navegação no Histórico

### Subir uma mensagem
```
Pressione: ↑
```

Indicador muda:
```
│ Mensagens - ⬆️ Manual     │
```

### Descer uma mensagem
```
Pressione: ↓
```

### Pular 10 mensagens
```
Pressione: PageUp    (sobe 10)
Pressione: PageDown  (desce 10)
```

### Ir para o início
```
Pressione: Home
```

### Ir para o final (reativar auto-scroll)
```
Pressione: End
```

Indicador volta:
```
│ Mensagens - 📍 Auto-scroll │
```

## 8. Observando Typing Indicators

Quando Bob está digitando:
```
┌────────────────────────────┬─────────────────────┐
│ Mensagens - 📍 Auto-scroll │ Online (3)          │
│                            │ • alice             │
│                            │ • bob ⌨️            │
│                            │ • charlie           │
└────────────────────────────┴─────────────────────┘
```

## 9. Recebendo Notificações

Quando você está na aba **Geral** e chega uma DM:

```
┌──────────────────────────────────────────────────┐
│ ⚠️ Aviso: Nova mensagem!                        │
├──────────────────────────────────────────────────┤
│  [Geral] DMs (1) Salas                          │
│                  ^^^
```

A notificação some automaticamente após 2 segundos.

## 10. Saindo de uma Sala

```
Digite: /leave
Pressione: Enter
```

Se não estiver em nenhuma sala:
```
┌──────────────────────────────────────────────────┐
│ ⚠️ Aviso: Você não está em nenhuma sala         │
└──────────────────────────────────────────────────┘
```

## 11. Personalizando Cores

Crie `colors.toml` na raiz do projeto:

```toml
[messages]
system = "Blue"
dm = "Red"
room = "Green"
broadcast = "White"

[ui]
border = "Cyan"
tabs_active = "Yellow"
tabs_inactive = "Gray"
notification = "Red"
input = "Green"

[users]
online = "Cyan"
typing = "Magenta"
```

Reinicie o cliente e veja as cores mudarem!

## 12. Cenários Completos

### Cenário 1: Conversa em Grupo

```bash
# Alice
alice> Pessoal, preciso de ajuda com Rust
alice> Alguém entende de lifetimes?

# Bob responde
[14:35] bob: Sim! O que você precisa?

# Alice envia DM
alice> /msg bob Pode me ajudar no privado?

# Tab para DMs
[Tab]
🤫 DM [14:36] bob: Pode me ajudar no privado?

# Bob responde no privado
🤫 DM [14:36] bob: Claro, qual é o problema?
```

### Cenário 2: Trabalhando em Sala

```bash
# Entrar na sala dev
alice> /join dev
[sistema] Entrou na sala dev

# Tab para Salas
[Tab] [Tab]

# Conversar na sala
alice> Bom dia time!
#dev [14:40] alice: Bom dia time!

# Charlie também entra
#dev [sistema] charlie entrou na sala dev
#dev [14:41] charlie: Bom dia!

# Sair da sala
alice> /leave
[sistema] Saiu da sala dev
```

### Cenário 3: Gerenciando Múltiplas Conversas

```bash
# Na aba Geral
[Geral] ✓
alice> Oi geral!

# Nova DM chega
[Notification] Nova mensagem!
[Geral] [DMs (1)] Salas
         ^^^^^^^^

# Tab para DMs
[Tab]
🤫 DM [14:45] bob: Preciso falar contigo

# Responder
alice> /msg bob Fala!

# Voltar para Geral
[Shift+Tab]

# Entrar em sala
alice> /join dev

# Tab para Salas
[Tab]
#dev [sistema] Entrou na sala dev

# Múltiplas abas ativas
[Geral (2)] [DMs (1)] [Salas] ✓
```

## 13. Dicas e Truques

### Dica 1: Monitorar Conversas
- Deixe na aba **Geral** para ver atividade geral
- Contadores `(N)` mostram onde há atividade
- Pressione `Tab` para ir direto para mensagens não lidas

### Dica 2: Buscar no Histórico
- Use `Home` para ir ao início
- `PageUp` para subir rápido
- `↑` para navegar com precisão
- `End` para voltar ao final

### Dica 3: Personalizar Experiência
- Edite `colors.toml` para seu tema favorito
- Experimente cores diferentes para cada tipo de mensagem
- Reinicie o cliente para aplicar mudanças

### Dica 4: Trabalhar com Salas
- Só pode estar em uma sala por vez
- Use `/leave` antes de `/join` outra
- Mensagens de sala aparecem com `#sala`

### Dica 5: Performance
- Histórico limitado a 500 mensagens (auto-limpa)
- Lista de usuários atualiza a cada 5s
- Typing indicators expiram em 2s

## 14. Atalhos Rápidos

```
Ctrl+C           → Sair imediatamente
Tab              → Próxima aba
Shift+Tab        → Aba anterior
End              → Voltar ao final (ativa auto-scroll)
Home             → Ir ao início
PageDown         → Pular 10 mensagens abaixo
PageUp           → Pular 10 mensagens acima
/msg <user> <msg> → DM rápido
/join <sala>     → Entrar em sala
/leave           → Sair de sala
```
