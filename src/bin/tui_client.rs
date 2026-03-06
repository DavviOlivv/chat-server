use std::collections::HashMap;
use std::fs;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex as AsyncMutex;

use chat_serve::server::listener;
use chat_serve::{AckKind, ChatAction, ChatMessage};
use chrono::{Local, Utc};
use rpassword::read_password;
use rustls_pki_types;
use serde::Deserialize;
use tracing::{error, info};
use uuid::Uuid;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs},
    Frame, Terminal,
};

#[derive(Clone, Debug, Deserialize)]
struct ColorConfig {
    messages: MessageColors,
    ui: UiColors,
    users: UserColors,
}

#[derive(Clone, Debug, Deserialize)]
struct MessageColors {
    system: String,
    dm: String,
    room: String,
    broadcast: String,
}

#[derive(Clone, Debug, Deserialize)]
struct UiColors {
    border: String,
    tabs_active: String,
    tabs_inactive: String,
    notification: String,
    input: String,
}

#[derive(Clone, Debug, Deserialize)]
struct UserColors {
    online: String,
    typing: String,
}

impl ColorConfig {
    fn load() -> Self {
        match fs::read_to_string("colors.toml") {
            Ok(content) => toml::from_str(&content).unwrap_or_else(|_| Self::default()),
            Err(_) => Self::default(),
        }
    }

    fn default() -> Self {
        Self {
            messages: MessageColors {
                system: "Cyan".to_string(),
                dm: "Magenta".to_string(),
                room: "Yellow".to_string(),
                broadcast: "White".to_string(),
            },
            ui: UiColors {
                border: "White".to_string(),
                tabs_active: "Green".to_string(),
                tabs_inactive: "Gray".to_string(),
                notification: "Red".to_string(),
                input: "White".to_string(),
            },
            users: UserColors {
                online: "Green".to_string(),
                typing: "Yellow".to_string(),
            },
        }
    }

    fn parse_color(s: &str) -> Color {
        match s {
            "Black" => Color::Black,
            "Red" => Color::Red,
            "Green" => Color::Green,
            "Yellow" => Color::Yellow,
            "Blue" => Color::Blue,
            "Magenta" => Color::Magenta,
            "Cyan" => Color::Cyan,
            "Gray" => Color::Gray,
            "White" => Color::White,
            _ => Color::White,
        }
    }
}

#[derive(Clone, Debug)]
struct Message {
    from: String,
    content: String,
    timestamp: String,
    is_dm: bool,
    is_system: bool,
    room: Option<String>,
}

#[derive(Clone)]
struct App {
    input: String,
    messages: Vec<Message>,
    users_online: Vec<String>,
    current_tab: usize,
    tabs: Vec<String>,
    scroll_offset: usize,
    username: String,
    typing_users: HashMap<String, Instant>,
    current_room: Option<String>,
    unread_counts: HashMap<usize, usize>, // tab_index -> unread_count
    auto_scroll: bool,
    show_notification: bool,
    notification_message: String,
    colors: ColorConfig,
}

impl App {
    fn new(username: String) -> Self {
        Self {
            input: String::new(),
            messages: Vec::new(),
            users_online: Vec::new(),
            current_tab: 0,
            tabs: vec!["Geral".to_string(), "DMs".to_string(), "Salas".to_string()],
            scroll_offset: 0,
            username,
            typing_users: HashMap::new(),
            current_room: None,
            unread_counts: HashMap::new(),
            auto_scroll: true,
            show_notification: false,
            notification_message: String::new(),
            colors: ColorConfig::load(),
        }
    }

    fn add_message(&mut self, msg: Message) {
        // Incrementar contador de não lidas se não estiver na aba correspondente
        let tab_index = if msg.is_dm {
            1 // DMs tab
        } else if msg.room.is_some() {
            2 // Salas tab
        } else {
            0 // Geral tab
        };

        let is_system = msg.is_system;

        if tab_index != self.current_tab && !is_system {
            *self.unread_counts.entry(tab_index).or_insert(0) += 1;
        }

        self.messages.push(msg);

        // Auto-scroll se habilitado
        if self.auto_scroll {
            self.scroll_to_bottom();
        }

        // Limpar mensagens antigas
        if self.messages.len() > 500 {
            self.messages.remove(0);
            if self.scroll_offset > 0 {
                self.scroll_offset -= 1;
            }
        }

        // Mostrar notificação visual
        if !is_system {
            self.show_notification = true;
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(2)).await;
            });
        }
    }

    fn filtered_messages(&self) -> Vec<&Message> {
        match self.current_tab {
            0 => self
                .messages
                .iter()
                .filter(|m| !m.is_dm && m.room.is_none())
                .collect(),
            1 => self.messages.iter().filter(|m| m.is_dm).collect(),
            2 => self.messages.iter().filter(|m| m.room.is_some()).collect(),
            _ => self.messages.iter().collect(),
        }
    }

    fn next_tab(&mut self) {
        self.current_tab = (self.current_tab + 1) % self.tabs.len();
        // Limpar contador de não lidas ao entrar na aba
        self.unread_counts.insert(self.current_tab, 0);
        self.scroll_to_bottom();
    }

    fn previous_tab(&mut self) {
        if self.current_tab > 0 {
            self.current_tab -= 1;
        } else {
            self.current_tab = self.tabs.len() - 1;
        }
        // Limpar contador de não lidas ao entrar na aba
        self.unread_counts.insert(self.current_tab, 0);
        self.scroll_to_bottom();
    }

    fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
            self.auto_scroll = false;
        }
    }

    fn scroll_down(&mut self) {
        let filtered_len = self.filtered_messages().len();
        if self.scroll_offset < filtered_len.saturating_sub(1) {
            self.scroll_offset += 1;
        }
        // Reativar auto-scroll se chegou no final
        if self.scroll_offset >= filtered_len.saturating_sub(1) {
            self.auto_scroll = true;
        }
    }

    fn page_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(10);
        self.auto_scroll = false;
    }

    fn page_down(&mut self) {
        let filtered_len = self.filtered_messages().len();
        self.scroll_offset = (self.scroll_offset + 10).min(filtered_len.saturating_sub(1));
        if self.scroll_offset >= filtered_len.saturating_sub(1) {
            self.auto_scroll = true;
        }
    }

    fn scroll_to_bottom(&mut self) {
        let filtered = self.filtered_messages();
        self.scroll_offset = filtered.len().saturating_sub(1);
        self.auto_scroll = true;
    }

    fn set_notification(&mut self, message: String) {
        self.notification_message = message;
        self.show_notification = true;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Inicializa logging
    let env_filter = tracing_subscriber::EnvFilter::from_default_env();
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    // Configurações TLS
    let tls_enabled = std::env::var("TLS_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let tls_insecure = std::env::var("TLS_INSECURE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let tls_ca_cert = std::env::var("TLS_CA_CERT").ok();

    let addr = std::env::var("SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());

    println!("=== Chat TUI Client ===\n");
    println!("Digite seu nome de usuário:");
    let mut username = String::new();
    io::stdin().read_line(&mut username)?;
    let username = username.trim().to_string();

    println!("Digite sua senha:");
    let password = read_password().unwrap_or_default();
    let password = password.trim().to_string();

    info!("Conectando ao servidor {}...", addr);

    // Conectar com ou sem TLS
    if tls_enabled {
        let connector = listener::load_tls_connector(tls_insecure, tls_ca_cert.as_deref())?;
        let tcp_stream = TcpStream::connect(&addr).await?;

        let domain = rustls_pki_types::ServerName::try_from("localhost")
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid server name"))?
            .to_owned();

        let tls_stream = connector.connect(domain, tcp_stream).await?;
        run_tui_client(tls_stream, username, password).await
    } else {
        let stream = TcpStream::connect(&addr).await?;
        run_tui_client(stream, username, password).await
    }
}

async fn run_tui_client<S>(
    stream: S,
    username: String,
    password: String,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (reader, writer) = tokio::io::split(stream);
    let writer = Arc::new(AsyncMutex::new(writer));
    let mut reader = BufReader::new(reader);

    // Login
    let login_msg = ChatMessage::Login {
        username: username.clone(),
        password: password.clone(),
    };
    let login_json = serde_json::to_string(&login_msg)?;
    {
        let mut w = writer.lock().await;
        w.write_all(format!("{}\n", login_json).as_bytes()).await?;
    }

    // Esperar resposta de login
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let login_response: ChatMessage = serde_json::from_str(&line)?;
    let session_token = match login_response {
        ChatMessage::SessionToken { token, .. } => Some(token),
        ChatMessage::Error(e) => {
            eprintln!("❌ Erro no login: {}", e);
            return Ok(());
        }
        _ => None,
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Estado compartilhado
    let app = Arc::new(AsyncMutex::new(App::new(username.clone())));
    let session_token = Arc::new(AsyncMutex::new(session_token));

    // Task para receber mensagens do servidor
    {
        let app = app.clone();
        let writer_clone = writer.clone();
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        if let Ok(msg) = serde_json::from_str::<ChatMessage>(&line) {
                            let mut app = app.lock().await;
                            match msg {
                                ChatMessage::Text {
                                    from,
                                    content,
                                    timestamp,
                                } => {
                                    let ts =
                                        timestamp.with_timezone(&Local).format("%H:%M").to_string();
                                    app.add_message(Message {
                                        from,
                                        content,
                                        timestamp: ts,
                                        is_dm: false,
                                        is_system: false,
                                        room: None,
                                    });
                                }
                                ChatMessage::Private {
                                    from,
                                    content,
                                    timestamp,
                                    message_id,
                                    ..
                                } => {
                                    let ts =
                                        timestamp.with_timezone(&Local).format("%H:%M").to_string();
                                    app.add_message(Message {
                                        from: from.clone(),
                                        content,
                                        timestamp: ts,
                                        is_dm: true,
                                        is_system: false,
                                        room: None,
                                    });

                                    // Enviar read receipt
                                    if let Some(msg_id) = message_id {
                                        let receipt = ChatMessage::ReadReceipt {
                                            message_id: msg_id,
                                            reader: app.username.clone(),
                                        };
                                        if let Ok(json) = serde_json::to_string(&receipt) {
                                            let mut w = writer_clone.lock().await;
                                            let _ =
                                                w.write_all(format!("{}\n", json).as_bytes()).await;
                                        }
                                    }
                                }
                                ChatMessage::Ack { kind, info, .. } => {
                                    let is_system = matches!(kind, AckKind::System);

                                    // Detectar notificações de sala
                                    let room = if info.contains("Entrou na sala")
                                        || info.contains("Saiu da sala")
                                    {
                                        info.split_whitespace().last().map(|s| s.to_string())
                                    } else {
                                        None
                                    };

                                    app.add_message(Message {
                                        from: "Sistema".to_string(),
                                        content: info,
                                        timestamp: Local::now().format("%H:%M").to_string(),
                                        is_dm: false,
                                        is_system,
                                        room,
                                    });
                                }
                                ChatMessage::ListResponse { users } => {
                                    app.users_online = users;
                                }
                                ChatMessage::Typing { user, typing } => {
                                    if typing {
                                        app.typing_users.insert(user, Instant::now());
                                    } else {
                                        app.typing_users.remove(&user);
                                    }
                                }
                                ChatMessage::SearchResponse { messages, total } => {
                                    if messages.is_empty() {
                                        app.set_notification(
                                            "🔍 Nenhum resultado encontrado".to_string(),
                                        );
                                    } else {
                                        let mut results =
                                            format!("🔍 Encontrados {} resultados:\n\n", total);
                                        for (idx, result) in messages.iter().enumerate() {
                                            results.push_str(&format!(
                                                "{}. [{}] {} → {}: {}\n   Relevância: {:.2}\n\n",
                                                idx + 1,
                                                result.timestamp,
                                                result.from_user,
                                                result
                                                    .to_user
                                                    .as_ref()
                                                    .unwrap_or(&"Broadcast".to_string()),
                                                result.snippet,
                                                result.rank
                                            ));
                                        }
                                        app.set_notification(results);
                                    }
                                }
                                ChatMessage::SessionToken { .. } => {
                                    // Já recebido antes do TUI
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(e) => {
                        error!("Erro ao ler do servidor: {}", e);
                        break;
                    }
                }
            }
        });
    }

    // Task para solicitar lista de usuários periodicamente
    {
        let writer = writer.clone();
        let session_token = session_token.clone();
        let username = username.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                let token = session_token.lock().await;
                if let Some(token) = token.as_ref() {
                    let list_msg = ChatMessage::Authenticated {
                        token: token.clone(),
                        action: Box::new(ChatAction::ListRequest {
                            from: username.clone(),
                        }),
                    };
                    if let Ok(json) = serde_json::to_string(&list_msg) {
                        let mut w = writer.lock().await;
                        let _ = w.write_all(format!("{}\n", json).as_bytes()).await;
                    }
                }
            }
        });
    }

    // Task para limpar typing indicators expirados
    {
        let app = app.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(500));
            loop {
                interval.tick().await;
                let mut app = app.lock().await;
                let now = Instant::now();
                app.typing_users
                    .retain(|_, last_seen| now.duration_since(*last_seen) < Duration::from_secs(2));
            }
        });
    }

    // Loop principal de UI
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();
    let mut last_typing_sent = None::<Instant>;
    let mut is_typing = false;

    loop {
        let app_state = app.lock().await.clone();
        drop(app_state); // Libera o lock antes de desenhar

        terminal.draw(|f| {
            let app_guard = app.try_lock();
            if let Ok(app) = app_guard {
                ui(f, &app);
            }
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let mut app = app.lock().await;

                    match key.code {
                        KeyCode::Char('c')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            break;
                        }
                        KeyCode::Tab => {
                            app.next_tab();
                        }
                        KeyCode::BackTab => {
                            app.previous_tab();
                        }
                        KeyCode::Up => {
                            app.scroll_up();
                        }
                        KeyCode::Down => {
                            app.scroll_down();
                        }
                        KeyCode::PageUp => {
                            app.page_up();
                        }
                        KeyCode::PageDown => {
                            app.page_down();
                        }
                        KeyCode::Home => {
                            app.scroll_offset = 0;
                            app.auto_scroll = false;
                        }
                        KeyCode::End => {
                            app.scroll_to_bottom();
                        }
                        KeyCode::Char(c) => {
                            app.input.push(c);

                            // Enviar typing indicator
                            if !is_typing {
                                is_typing = true;
                                let typing_msg = ChatMessage::Typing {
                                    user: username.clone(),
                                    typing: true,
                                };
                                if let Ok(json) = serde_json::to_string(&typing_msg) {
                                    let writer_clone = writer.clone();
                                    tokio::spawn(async move {
                                        let mut w = writer_clone.lock().await;
                                        let _ = w.write_all(format!("{}\n", json).as_bytes()).await;
                                    });
                                }
                            }
                            last_typing_sent = Some(Instant::now());
                        }
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::Enter => {
                            let input = app.input.drain(..).collect::<String>();

                            if !input.is_empty() {
                                let token_guard = session_token.lock().await;
                                if let Some(token) = token_guard.as_ref() {
                                    let msg = if input.starts_with("/msg ") {
                                        // DM: /msg <user> <message>
                                        let parts: Vec<&str> = input.splitn(3, ' ').collect();
                                        if parts.len() == 3 {
                                            Some(ChatMessage::Authenticated {
                                                token: token.clone(),
                                                action: Box::new(ChatAction::Private {
                                                    from: username.clone(),
                                                    to: parts[1].to_string(),
                                                    content: parts[2].to_string(),
                                                    timestamp: Utc::now(),
                                                    message_id: Some(Uuid::new_v4().to_string()),
                                                }),
                                            })
                                        } else {
                                            app.set_notification(
                                                "Uso: /msg <usuario> <mensagem>".to_string(),
                                            );
                                            None
                                        }
                                    } else if input.starts_with("/join ") {
                                        // Join room: /join <room>
                                        let room = input[6..].trim().to_string();
                                        if !room.is_empty() {
                                            app.current_room = Some(room.clone());
                                            Some(ChatMessage::Authenticated {
                                                token: token.clone(),
                                                action: Box::new(ChatAction::JoinRoom { room }),
                                            })
                                        } else {
                                            app.set_notification("Uso: /join <sala>".to_string());
                                            None
                                        }
                                    } else if input.starts_with("/leave") {
                                        // Leave room
                                        if let Some(room) = app.current_room.clone() {
                                            app.current_room = None;
                                            Some(ChatMessage::Authenticated {
                                                token: token.clone(),
                                                action: Box::new(ChatAction::LeaveRoom { room }),
                                            })
                                        } else {
                                            app.set_notification(
                                                "Você não está em nenhuma sala".to_string(),
                                            );
                                            None
                                        }
                                    } else if input.starts_with("/search ") {
                                        // Search messages: /search <query> [from:user]
                                        let parts: Vec<&str> = input.splitn(2, ' ').collect();
                                        if parts.len() == 2 {
                                            let search_input = parts[1];
                                            let (query, user_filter) = if let Some(from_idx) =
                                                search_input.find("from:")
                                            {
                                                let query_part = search_input[..from_idx].trim();
                                                let user_part = search_input[from_idx + 5..].trim();
                                                (
                                                    query_part.to_string(),
                                                    Some(user_part.to_string()),
                                                )
                                            } else {
                                                (search_input.to_string(), None)
                                            };

                                            if !query.is_empty() {
                                                Some(ChatMessage::Authenticated {
                                                    token: token.clone(),
                                                    action: Box::new(ChatAction::SearchMessages {
                                                        query,
                                                        limit: Some(50),
                                                        user_filter,
                                                    }),
                                                })
                                            } else {
                                                app.set_notification(
                                                    "Uso: /search <query> [from:user]".to_string(),
                                                );
                                                None
                                            }
                                        } else {
                                            app.set_notification(
                                                "Uso: /search <query> [from:user]".to_string(),
                                            );
                                            None
                                        }
                                    } else {
                                        // Mensagem broadcast
                                        Some(ChatMessage::Authenticated {
                                            token: token.clone(),
                                            action: Box::new(ChatAction::Text {
                                                from: username.clone(),
                                                content: input,
                                                timestamp: Utc::now(),
                                            }),
                                        })
                                    };

                                    if let Some(msg) = msg {
                                        if let Ok(json) = serde_json::to_string(&msg) {
                                            let writer_clone = writer.clone();
                                            tokio::spawn(async move {
                                                let mut w = writer_clone.lock().await;
                                                let _ = w
                                                    .write_all(format!("{}\n", json).as_bytes())
                                                    .await;
                                            });
                                        }
                                    }
                                }
                            }

                            // Parar typing indicator
                            is_typing = false;
                            let typing_msg = ChatMessage::Typing {
                                user: username.clone(),
                                typing: false,
                            };
                            if let Ok(json) = serde_json::to_string(&typing_msg) {
                                let writer_clone = writer.clone();
                                tokio::spawn(async move {
                                    let mut w = writer_clone.lock().await;
                                    let _ = w.write_all(format!("{}\n", json).as_bytes()).await;
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Auto-stop typing após debounce
        if is_typing {
            if let Some(last) = last_typing_sent {
                if Instant::now().duration_since(last) >= Duration::from_millis(500) {
                    is_typing = false;
                    let typing_msg = ChatMessage::Typing {
                        user: username.clone(),
                        typing: false,
                    };
                    if let Ok(json) = serde_json::to_string(&typing_msg) {
                        let writer_clone = writer.clone();
                        tokio::spawn(async move {
                            let mut w = writer_clone.lock().await;
                            let _ = w.write_all(format!("{}\n", json).as_bytes()).await;
                        });
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    // Restaurar terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut Frame, app: &App) {
    let size = f.size();

    // Layout principal: [Notification, Tabs, Content, Input]
    let mut constraints = vec![
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(3),
    ];

    if app.show_notification && !app.notification_message.is_empty() {
        constraints.insert(0, Constraint::Length(3));
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(size);

    let mut chunk_offset = 0;

    // Notificação (se ativa)
    if app.show_notification && !app.notification_message.is_empty() {
        let notification = Paragraph::new(app.notification_message.as_str())
            .style(
                Style::default()
                    .fg(ColorConfig::parse_color(&app.colors.ui.notification))
                    .add_modifier(Modifier::BOLD),
            )
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(
                        Style::default().fg(ColorConfig::parse_color(&app.colors.ui.border)),
                    )
                    .title("⚠️ Aviso"),
            );
        f.render_widget(notification, chunks[0]);
        chunk_offset = 1;
    }

    // Tabs com contadores de não lidas
    let tab_titles: Vec<Line> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let unread = app.unread_counts.get(&i).copied().unwrap_or(0);
            if unread > 0 {
                Line::from(format!("{} ({})", t, unread))
            } else {
                Line::from(t.as_str())
            }
        })
        .collect();
    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ColorConfig::parse_color(&app.colors.ui.border)))
                .title("Chat"),
        )
        .select(app.current_tab)
        .style(Style::default().fg(ColorConfig::parse_color(&app.colors.ui.tabs_inactive)))
        .highlight_style(
            Style::default()
                .fg(ColorConfig::parse_color(&app.colors.ui.tabs_active))
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, chunks[chunk_offset]);

    // Content area: [Messages, Users]
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
        .split(chunks[chunk_offset + 1]);

    // Messages (filtradas por aba)
    let filtered_messages = app.filtered_messages();
    let messages: Vec<ListItem> = filtered_messages
        .iter()
        .map(|m| {
            let color = if m.is_system {
                ColorConfig::parse_color(&app.colors.messages.system)
            } else if m.is_dm {
                ColorConfig::parse_color(&app.colors.messages.dm)
            } else if m.room.is_some() {
                ColorConfig::parse_color(&app.colors.messages.room)
            } else {
                ColorConfig::parse_color(&app.colors.messages.broadcast)
            };
            let style = Style::default().fg(color);

            let prefix = if m.is_dm {
                "🤫 DM"
            } else if let Some(ref room) = m.room {
                &format!("#{}", room)
            } else {
                ""
            };

            let content = if !prefix.is_empty() {
                format!("[{}] {} {}: {}", m.timestamp, prefix, m.from, m.content)
            } else {
                format!("[{}] {}: {}", m.timestamp, m.from, m.content)
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let scroll_indicator = if app.auto_scroll {
        "📍 Auto-scroll"
    } else {
        "⬆️ Manual"
    };

    let messages_widget = List::new(messages).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ColorConfig::parse_color(&app.colors.ui.border)))
            .title(format!("Mensagens - {}", scroll_indicator)),
    );
    f.render_widget(messages_widget, content_chunks[0]);

    // Users online + typing indicators
    let mut user_items: Vec<ListItem> = app
        .users_online
        .iter()
        .map(|u| {
            let (typing_indicator, color) = if app.typing_users.contains_key(u) {
                (" ⌨️", ColorConfig::parse_color(&app.colors.users.typing))
            } else {
                ("", ColorConfig::parse_color(&app.colors.users.online))
            };
            ListItem::new(format!("• {}{}", u, typing_indicator)).style(Style::default().fg(color))
        })
        .collect();

    // Adicionar header
    user_items.insert(
        0,
        ListItem::new(format!("Online ({})", app.users_online.len())).style(
            Style::default()
                .fg(ColorConfig::parse_color(&app.colors.ui.tabs_active))
                .add_modifier(Modifier::BOLD),
        ),
    );

    let users_widget = List::new(user_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(ColorConfig::parse_color(&app.colors.ui.border)))
            .title("Usuários"),
    );
    f.render_widget(users_widget, content_chunks[1]);

    // Input
    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(ColorConfig::parse_color(&app.colors.ui.input)))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(ColorConfig::parse_color(&app.colors.ui.border)))
                .title("Input (Ctrl+C para sair)"),
        );
    f.render_widget(input, chunks[chunks.len() - 1]);
}
