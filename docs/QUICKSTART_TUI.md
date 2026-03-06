# 🚀 Quick Start - Cliente TUI

## Em 3 Passos

### 1. Compilar
```bash
cargo build --bin tui
```

### 2. Executar o Servidor (em outro terminal)
```bash
cargo run --bin chat_server
```

### 3. Executar o Cliente TUI
```bash
cargo run --bin tui
```

## Primeira Vez Usando

```
┌─────────────────────────────────┐
│ Digite seu username: alice      │
│ Digite sua senha: ••••••        │
└─────────────────────────────────┘
```

## Interface

```
┌──────────────────────────────────────────────────┐
│  [Geral] DMs Salas                     ← 3 Abas │
├────────────────────────────┬─────────────────────┤
│ Mensagens - 📍 Auto-scroll │ Online (1)          │
│                            │ • alice             │
│ [sistema] Bem-vindo!       │                     │
│                            │                     │
├────────────────────────────┴─────────────────────┤
│ Input (Ctrl+C para sair)                        │
│ _                                               │
└──────────────────────────────────────────────────┘
```

## Comandos Básicos

| O que fazer | Como fazer |
|-------------|------------|
| **Mensagem pública** | Digite e pressione Enter |
| **Mensagem privada** | `/msg bob Olá!` |
| **Entrar em sala** | `/join dev` |
| **Sair de sala** | `/leave` |
| **Trocar aba** | Tab / Shift+Tab |
| **Navegar histórico** | ↑ ↓ PageUp PageDown |
| **Sair** | Ctrl+C |

## Recursos Principais

✅ **3 Abas Filtradas**
- **Geral**: Broadcasts e sistema
- **DMs**: Mensagens privadas
- **Salas**: Chat rooms

✅ **Notificações Visuais**
- Contador `(N)` em abas com não lidas
- Barra de aviso para eventos importantes

✅ **Navegação Inteligente**
- Auto-scroll acompanha mensagens
- PageUp/PageDown para histórico
- Indicador visual de modo

✅ **Cores Personalizáveis**
- Crie `colors.toml` na raiz
- Customize todas as cores
- Recarrega automaticamente

## Exemplo Rápido

```bash
# Terminal 1: Servidor
$ cargo run --bin chat_server
Servidor rodando em 127.0.0.1:8080

# Terminal 2: Cliente TUI (Alice)
$ cargo run --bin tui
username: alice
password: ••••••

alice> Olá pessoal!
[14:30] alice: Olá pessoal!

alice> /join dev
[sistema] Entrou na sala dev

alice> Preciso de ajuda com Rust
#dev [14:31] alice: Preciso de ajuda com Rust

# Terminal 3: Cliente TUI (Bob)
$ cargo run --bin tui
username: bob
password: ••••••

[14:30] alice: Olá pessoal!

bob> /msg alice Posso ajudar!
🤫 DM [14:32] bob: Posso ajudar!

# Alice vê notificação
[Geral] [DMs (1)] [Salas]
         ^^^^^^^
```

## Próximos Passos

📖 Documentação completa: [TUI_CLIENT.md](TUI_CLIENT.md)  
🎯 Exemplos de uso: [TUI_EXAMPLE.md](TUI_EXAMPLE.md)  
🎨 Configurar cores: [colors.toml](colors.toml)

## Troubleshooting

### Servidor não conecta
```bash
# Verificar se porta 8080 está livre
lsof -i :8080

# Mudar porta no código se necessário
```

### Cores não aparecem
```bash
# Seu terminal precisa suportar cores ANSI
# Testado em: iTerm2, Terminal.app, Alacritty

# Verificar suporte
echo -e "\033[31mRed\033[0m"
```

### Layout quebrado
```bash
# Terminal muito pequeno
# Mínimo recomendado: 80x24

# Verificar tamanho
tput cols  # largura
tput lines # altura
```

## Ajuda

- GitHub Issues: [chat-serve/issues](https://github.com/seu-usuario/chat-serve/issues)
- Documentação: Ver arquivos `.md` na raiz do projeto
- Exemplos: Ver `TUI_EXAMPLE.md` para casos de uso completos
