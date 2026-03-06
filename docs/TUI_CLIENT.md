# Cliente TUI com Ratatui

Interface gráfica moderna no terminal para o chat-serve.

## 🎨 Features Implementadas

### ✅ Interface Moderna
- **3 Abas**: Geral (broadcast), DMs (mensagens privadas), Salas (chat rooms)
- **Filtro por Aba**: Cada aba mostra apenas mensagens relevantes
- **Contadores de Não Lidas**: Mostra `(N)` nas abas com mensagens não lidas
- **Notificações Visuais**: Barra de aviso no topo quando novas mensagens chegam
- **Painel de Usuários**: Lista usuários online com indicador de digitação ⌨️

### ✅ Navegação Inteligente
- **Auto-scroll**: Acompanha automaticamente novas mensagens
- **Navegação Manual**: Use setas, PageUp/PageDown, Home/End
- **Indicador Visual**: Mostra 📍 Auto-scroll ou ⬆️ Manual
- **Debounce de Digitação**: Envia indicador de digitação após 500ms sem input

### ✅ Comandos Avançados
- `/msg <usuário> <mensagem>` - Enviar DM privada
- `/join <sala>` - Entrar em uma sala de chat
- `/leave` - Sair da sala atual
- Enter sem comando - Broadcast para todos (ou para sala se estiver em uma)

### ✅ Personalização
- **Cores Configuráveis**: Arquivo `colors.toml` permite customizar todas as cores
- **Cores Suportadas**: Black, Red, Green, Yellow, Blue, Magenta, Cyan, Gray, White

## 🎨 Configuração de Cores

Crie um arquivo `colors.toml` na raiz do projeto:

```toml
# Cores das mensagens
[messages]
system = "Cyan"        # Mensagens do sistema
dm = "Magenta"        # Mensagens privadas
room = "Yellow"       # Mensagens de salas
broadcast = "White"   # Mensagens broadcast

# Cores da interface
[ui]
border = "White"          # Bordas dos painéis
tabs_active = "Green"     # Aba ativa
tabs_inactive = "Gray"    # Abas inativas
notification = "Red"      # Barra de notificações
input = "White"          # Campo de input

# Cores dos usuários
[users]
online = "Green"      # Usuários online
typing = "Yellow"     # Usuários digitando
```

Se o arquivo não existir, cores padrão serão usadas.

## Como Usar

### Executar

```bash
cargo run --bin tui
```

### Atalhos de Teclado

| Tecla | Ação |
|-------|------|
| `Ctrl+C` | Sair |
| `Tab` | Próxima aba (limpa contador de não lidas) |
| `Shift+Tab` | Aba anterior |
| `↑` | Scroll up (desativa auto-scroll) |
| `↓` | Scroll down (reativa auto-scroll no final) |
| `PageUp` | Navegar 10 mensagens acima |
| `PageDown` | Navegar 10 mensagens abaixo |
| `Home` | Ir para o início |
| `End` | Ir para o final (ativa auto-scroll) |
| `Enter` | Enviar mensagem ou executar comando |
| `Backspace` | Apagar caractere |

### Comandos

| Comando | Descrição | Exemplo |
|---------|-----------|---------|
| (texto) | Broadcast para todos ou sala atual | `Olá pessoal!` |
| `/msg <usuário> <msg>` | Enviar mensagem privada | `/msg alice Olá!` |
| `/join <sala>` | Entrar em uma sala | `/join dev` |
| `/leave` | Sair da sala atual | `/leave` |

## Layout

```
┌──────────────────────────────────────────────────┐
│  [Geral] [DMs] [Salas]                          │ ← Tabs
├────────────────────────────┬─────────────────────┤
│ [10:30] alice: Olá todos!  │ Online (3)          │
│ 🤫 DM bob: mensagem secreta│ • alice             │
│ [10:31] Sistema: info...   │ • bob ⌨️            │ ← Typing
│                            │ • charlie           │
│                            │                     │
│                            │                     │
├────────────────────────────┴─────────────────────┤
│ Input (Ctrl+C para sair)                        │
│ /msg bob olá_                                   │ ← Input box
└──────────────────────────────────────────────────┘
```

## Layout Completo com Notificações

Quando há novas mensagens, uma barra de notificação aparece:

```
┌──────────────────────────────────────────────────┐
│ ⚠️ Aviso: Nova mensagem de alice!              │ ← Notificação
├──────────────────────────────────────────────────┤
│  [Geral] [DMs (2)] [Salas]                     │ ← Tabs c/ contador
├────────────────────────────┬─────────────────────┤
│ Mensagens - 📍 Auto-scroll │ Usuários            │ ← Indicador scroll
│ ...                        │ ...                 │
└────────────────────────────┴─────────────────────┘
```

## Diferenças do Cliente Original

| Recurso | Cliente Original | Cliente TUI |
|---------|------------------|-------------|
| Interface | stdout/stdin simples | Terminal UI com widgets |
| Usuários online | Comando `/list` | Painel sempre visível |
| Navegação | Linear | Abas + Scroll inteligente |
| Typing indicators | Texto simples | Ícone ⌨️ ao lado do nome |
| Cores | ANSI colors fixas | Configuráveis via TOML |
| Filtros | Nenhum | Por tipo de mensagem |
| Notificações | Inline | Barra dedicada |
| Salas | Não suportado | `/join` e `/leave` |
| UX | CLI básico | App-like experience |

## Melhorias de Usabilidade

### 1. Filtro Inteligente por Aba
- **Geral**: Apenas broadcasts e mensagens do sistema
- **DMs**: Apenas mensagens privadas enviadas/recebidas
- **Salas**: Apenas mensagens da sala atual

### 2. Sistema de Notificações
- Contadores nas abas mostram mensagens não lidas
- Barra de notificação temporária (2s) para eventos importantes
- Contadores são zerados ao entrar na aba

### 3. Navegação Eficiente
- Auto-scroll mantém você nas últimas mensagens
- Desativa automaticamente ao rolar manualmente
- Reativa ao chegar no final ou pressionar End
- PageUp/PageDown para navegação rápida

### 4. Personalização Visual
- Todas as cores configuráveis via `colors.toml`
- Cores diferentes para cada tipo de mensagem
- Indicadores visuais claros (emojis, símbolos)

## Arquitetura Técnica

### Estrutura de Dados

```rust
struct App {
    current_tab: usize,           // 0=Geral, 1=DMs, 2=Salas
    unread_counts: HashMap<usize, usize>,
    auto_scroll: bool,
    show_notification: bool,
    notification_message: String,
    current_room: Option<String>,
    colors: ColorConfig,          // Carregado de colors.toml
    // ... outros campos
}
```

### Tarefas Assíncronas

O cliente roda 4 tarefas concorrentes:

1. **UI Thread** - Renderização e input do teclado (loop principal)
2. **Network Receiver** - Recebe mensagens do servidor via TCP
3. **User List Refresh** - Atualiza lista a cada 5 segundos
4. **Typing Cleanup** - Remove indicadores expirados (500ms)

### Protocolo de Mensagens

Usa o mesmo protocolo JSON do servidor:

- `ChatMessage::Text` - Broadcast
- `ChatMessage::Private` - DMs
- `ChatMessage::JoinRoom` / `ChatMessage::LeaveRoom` - Salas
- `ChatMessage::Typing` - Indicadores de digitação
- `ChatMessage::ReadReceipt` - Confirmações de leitura
