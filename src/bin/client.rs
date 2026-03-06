use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{self, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex as AsyncMutex;

use chat_serve::server::listener;
use chat_serve::AckKind;
use chat_serve::ChatAction;
use chat_serve::ChatMessage; // Import via re-export from the library root
use chrono::{Local, Utc};
use colored::Colorize;
use rpassword::read_password;
use std::fs;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Inicializa logging (usar RUST_LOG=debug para ver todos os logs)
    let env_filter = tracing_subscriber::EnvFilter::from_default_env();
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    // Configurações TLS via env vars
    let tls_enabled = std::env::var("TLS_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let tls_insecure = std::env::var("TLS_INSECURE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let tls_ca_cert = std::env::var("TLS_CA_CERT").ok();

    let addr = std::env::var("SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    debug!("Tentando conectar a {}", addr);

    // Conectar com ou sem TLS
    if tls_enabled {
        info!("🔒 Conectando com TLS...");
        let connector = listener::load_tls_connector(tls_insecure, tls_ca_cert.as_deref())?;
        let tcp_stream = TcpStream::connect(&addr).await?;

        let domain = rustls_pki_types::ServerName::try_from("localhost")
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid server name"))?
            .to_owned();

        match connector.connect(domain, tcp_stream).await {
            Ok(tls_stream) => {
                info!("✅ Conectado ao servidor TLS em {}", addr);
                run_client(tls_stream).await
            }
            Err(e) => {
                error!("❌ Falha no handshake TLS: {}", e);
                Err(e.into())
            }
        }
    } else {
        info!("⚠️  Conectando sem TLS (modo inseguro)...");
        match TcpStream::connect(&addr).await {
            Ok(stream) => {
                info!("✅ Conectado ao servidor em {}", addr);
                run_client(stream).await
            }
            Err(e) => {
                error!("❌ Falha ao conectar a {}: {}", addr, e);
                Err(e.into())
            }
        }
    }
}

/// Executa a lógica principal do cliente com qualquer stream (TCP ou TLS)
async fn run_client<S>(stream: S) -> Result<(), Box<dyn std::error::Error>>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (reader, writer) = tokio::io::split(stream);
    // Compartilhamos o writer entre tarefas usando AsyncMutex
    let writer = Arc::new(AsyncMutex::new(writer));
    let mut reader = BufReader::new(reader);
    let mut stdin = BufReader::new(io::stdin());

    // 1. Fase de Login Simples
    info!("Digite seu nome de usuário:");
    let mut username = String::new();
    stdin.read_line(&mut username).await?;
    let username = username.trim().to_string();

    info!("Digite sua senha:");
    // Lemos a senha sem echo para melhor UX/security
    let password = read_password().unwrap_or_default();
    let password = password.trim().to_string();

    let login_msg = ChatMessage::Login {
        username: username.clone(),
        password: password.clone(),
    };
    let login_json = match serde_json::to_string(&login_msg) {
        Ok(j) => j,
        Err(e) => {
            error!("❌ Falha ao serializar mensagem de login: {}", e);
            return Err(e.into());
        }
    };
    debug!("Enviando login como: {}", username);
    {
        let mut w = writer.lock().await;
        if let Err(e) = w.write_all(format!("{}\n", login_json).as_bytes()).await {
            error!("❌ Falha ao enviar mensagem de login: {}", e);
            return Err(e.into());
        }
    }
    info!("✅ Login enviado para o servidor");

    // session token recebido após login
    let mut session_token: Option<String> = None;

    // 2. Loop Principal (Leitura e Escrita Simultâneas)
    let mut server_line = String::new();
    let mut user_line = String::new();

    // pending: message_id -> (ChatMessage, last_sent_instant, attempts)
    type PendingMessages = HashMap<String, (ChatMessage, Instant, u8)>;
    let pending: Arc<AsyncMutex<PendingMessages>> = Arc::new(AsyncMutex::new(HashMap::new()));
    // Histórico local das últimas mensagens (para ordenar por timestamp)
    let history: Arc<AsyncMutex<Vec<ChatMessage>>> = Arc::new(AsyncMutex::new(Vec::new()));

    // Typing indicator state
    let last_typing_sent: Arc<AsyncMutex<Option<Instant>>> = Arc::new(AsyncMutex::new(None));
    let is_typing: Arc<AsyncMutex<bool>> = Arc::new(AsyncMutex::new(false));

    // Tarefa de retry: reenvia DMs não confirmadas
    {
        // Configuração: prioridade CLI args > env vars > defaults
        let mut retry_secs_arg: Option<u64> = None;
        let mut max_attempts_arg: Option<u8> = None;
        let mut iter = std::env::args().skip(1);
        while let Some(a) = iter.next() {
            match a.as_str() {
                "--retry-interval" | "--retry-interval-secs" => {
                    if let Some(v) = iter.next() {
                        if let Ok(n) = v.parse::<u64>() {
                            retry_secs_arg = Some(n);
                        }
                    }
                }
                "--retry-attempts" | "--retry-max-attempts" => {
                    if let Some(v) = iter.next() {
                        if let Ok(n) = v.parse::<u8>() {
                            max_attempts_arg = Some(n);
                        }
                    }
                }
                _ => { /* ignore other args */ }
            }
        }

        let retry_secs = retry_secs_arg
            .or_else(|| {
                std::env::var("RETRY_INTERVAL_SECS")
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .unwrap_or(3u64);
        let max_attempts_cfg = max_attempts_arg
            .or_else(|| {
                std::env::var("RETRY_MAX_ATTEMPTS")
                    .ok()
                    .and_then(|s| s.parse::<u8>().ok())
            })
            .unwrap_or(3u8);

        info!(retry_interval_secs=%retry_secs, max_attempts=%max_attempts_cfg, "⚙️ Retry config");

        let writer = writer.clone();
        let pending = pending.clone();
        tokio::spawn(async move {
            let retry_interval = Duration::from_secs(retry_secs);
            let max_attempts = max_attempts_cfg;
            loop {
                tokio::time::sleep(retry_interval).await;
                let now = Instant::now();

                // Coleta ids para reenviar e para falhar
                let mut to_resend: Vec<String> = Vec::new();
                let mut to_fail: Vec<String> = Vec::new();

                {
                    let map = pending.lock().await;
                    for (id, (_msg, last, attempts)) in map.iter() {
                        if now.duration_since(*last) >= retry_interval {
                            if *attempts >= max_attempts {
                                to_fail.push(id.clone());
                            } else {
                                to_resend.push(id.clone());
                            }
                        }
                    }
                }

                // Marcar falhas
                if !to_fail.is_empty() {
                    let mut map = pending.lock().await;
                    for id in to_fail.drain(..) {
                        if let Some((msg, _last, attempts)) = map.remove(&id) {
                            match msg {
                                ChatMessage::Private { to, content, .. } => {
                                    error!(to=%to, message=%content, message_id=%id, attempts=%attempts, "❌ Falha definitiva ao entregar DM após {} tentativas", attempts);
                                    eprintln!("{} Não foi possível entregar mensagem para {} após {} tentativas: {}", "❌".red().bold(), to.red(), attempts, content.dimmed());
                                }
                                _ => {
                                    error!(message_id=%id, attempts=%attempts, "❌ Falha ao entregar mensagem após {} tentativas", attempts);
                                }
                            }
                        }
                    }
                }

                // Reenviar pendentes
                for id in to_resend.drain(..) {
                    // Atualiza attempts e timestamp, e reenvia
                    let maybe_pair = {
                        let mut map = pending.lock().await;
                        if let Some((msg_ref, last_ref, attempts_ref)) = map.get_mut(&id) {
                            *attempts_ref += 1;
                            let attempts_now = *attempts_ref;
                            let msg_clone = msg_ref.clone();
                            *last_ref = Instant::now();
                            Some((msg_clone, attempts_now))
                        } else {
                            None
                        }
                    };

                    if let Some((msg, attempts_now)) = maybe_pair {
                        // Notificação de retry para o usuário
                        match &msg {
                            ChatMessage::Private { to, content, .. } => {
                                warn!(
                                    "🔄 Reenviando ({}/{}) DM para {}: {}",
                                    attempts_now, max_attempts, to, content
                                );
                                debug!(message_id=%id, to=%to, attempts=%attempts_now, "Tentativa de reenvio");
                            }
                            _ => {
                                warn!(
                                    "🔄 Reenviando ({}/{}) mensagem id={}",
                                    attempts_now, max_attempts, id
                                );
                            }
                        }

                        let json = match serde_json::to_string(&msg) {
                            Ok(j) => j,
                            Err(e) => {
                                error!(message_id=%id, error=%e, "❌ Falha ao serializar mensagem para retry");
                                continue;
                            }
                        };
                        let mut w = writer.lock().await;
                        if let Err(e) = w.write_all(format!("{}\n", json).as_bytes()).await {
                            error!(message_id=%id, error=%e, "❌ Falha ao enviar retry pela rede");
                        } else {
                            debug!(message_id=%id, "✅ Retry enviado pela rede");
                        }
                    }
                }
            }
        });
    }

    // Tarefa de typing debounce: para typing indicator após 500ms sem input
    {
        let is_typing = is_typing.clone();
        let last_typing_sent = last_typing_sent.clone();
        let writer = writer.clone();
        let username_clone = username.clone();
        tokio::spawn(async move {
            let debounce_duration = Duration::from_millis(500);
            loop {
                tokio::time::sleep(Duration::from_millis(100)).await;

                let should_stop = {
                    let typing = is_typing.lock().await;
                    let last_sent = last_typing_sent.lock().await;

                    if *typing {
                        if let Some(last) = *last_sent {
                            Instant::now().duration_since(last) >= debounce_duration
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                };

                if should_stop {
                    // Enviar typing=false
                    let typing_msg = ChatMessage::Typing {
                        user: username_clone.clone(),
                        typing: false,
                    };
                    if let Ok(json) = serde_json::to_string(&typing_msg) {
                        let mut w = writer.lock().await;
                        let _ = w.write_all(format!("{}\n", json).as_bytes()).await;
                    }

                    // Atualizar estado
                    let mut typing = is_typing.lock().await;
                    *typing = false;
                }
            }
        });
    }

    loop {
        tokio::select! {
            // Ouvindo o Servidor
            res = reader.read_line(&mut server_line) => {
                match res {
                    Ok(0) => {
                        warn!("⚠️ Servidor fechou a conexão");
                        break;
                    }
                    Ok(_) => {},
                    Err(e) => {
                        error!("❌ Erro ao ler linha do servidor: {}", e);
                        break;
                    }
                }
                match serde_json::from_str::<ChatMessage>(&server_line) {
                    Ok(msg) => {
                    // Armazena no histórico (clona antes de mover para match)
                    let store_msg = msg.clone();
                    {
                        let mut h = history.lock().await;
                        h.push(store_msg);
                        if h.len() > 200 { h.remove(0); }
                    }
                    match msg {
                        ChatMessage::Text { from, content, timestamp } => {
                            let local_ts = timestamp.with_timezone(&Local);
                            let time_str = local_ts.format("%H:%M").to_string();
                            println!("[{}] {}: {}", time_str.dimmed(), from.green().bold(), content);
                        }
                        ChatMessage::Private { from, to: _, content, timestamp, message_id } => {
                            let local_ts = timestamp.with_timezone(&Local);
                            let time_str = local_ts.format("%H:%M").to_string();
                            println!("[{}] {} {} {}", time_str.dimmed(), "🤫 [DM de".magenta(), from.magenta().bold(), "]:".magenta());
                            println!("{}", content.italic().white());

                            // Enviar read receipt se temos message_id e session_token
                            if let (Some(msg_id), Some(ref _token)) = (message_id, &session_token) {
                                let receipt = ChatMessage::ReadReceipt {
                                    message_id: msg_id,
                                    reader: username.clone(),
                                };
                                let receipt_json = serde_json::to_string(&receipt).unwrap();
                                let writer_clone = writer.clone();
                                tokio::spawn(async move {
                                    let mut w = writer_clone.lock().await;
                                    if let Err(e) = w.write_all(format!("{}\n", receipt_json).as_bytes()).await {
                                        debug!("❌ Erro ao enviar read receipt: {}", e);
                                    }
                                });
                            }
                        }
                        ChatMessage::Ack { kind, info, message_id } => {
                            // Mapeamento simples de estilos por AckKind
                            let (icon, styled_info) = match kind {
                                AckKind::Delivered => ("✓✓".green().to_string(), info.yellow().to_string()),
                                AckKind::Received => {
                                    // Mensagens para offline têm indicador especial
                                    if info.contains("voltar online") || info.contains("quando voltar") {
                                        ("📭".yellow().to_string(), info.yellow().dimmed().to_string())
                                    } else {
                                        ("✓".green().to_string(), info.yellow().to_string())
                                    }
                                },
                                AckKind::Read => ("👁️".yellow().to_string(), info.white().to_string()),
                                AckKind::Failed => ("❌".red().to_string(), info.red().to_string()),
                                AckKind::System => {
                                    // Notificação especial para mensagens offline
                                    if info.contains("mensagens novas recebidas") || info.contains("mensagens offline") {
                                        println!("\n{}", "═".repeat(60).cyan());
                                        (("📬".to_string()), info.cyan().bold().to_string())
                                    } else {
                                        ("⚙️".cyan().to_string(), info.magenta().to_string())
                                    }
                                },
                            };

                            println!("{} {}", icon, styled_info);

                            // Adicionar linha separadora após notificação de mensagens offline
                            if matches!(kind, AckKind::System) && (info.contains("mensagens novas recebidas") || info.contains("mensagens offline")) {
                                println!("{}", "─".repeat(60).dimmed());
                            }

                            // Tratamento especial: rate limiting
                            if matches!(kind, AckKind::Failed) && info.contains("Limite de mensagens excedido") {
                                println!(
                                    "{} {}",
                                    "⏱ Rate limit:".yellow().bold(),
                                    "Você está enviando mensagens rápido demais. Aguarde um pouco antes de tentar novamente.".yellow()
                                );
                            }

                            // Se vier com message_id, remove das pendentes
                            // Se o Ack indicar token inválido/expirado, limpamos o token local
                            if matches!(kind, AckKind::Failed) && info.to_lowercase().contains("token inválido") || info.to_lowercase().contains("expirad") {
                                session_token = None;
                                println!("{} Seu token de sessão é inválido/expirado. Por favor, faça login novamente.", "⚠️".yellow());
                            }

                            // Se vier com message_id, remove das pendentes
                            if let Some(id) = message_id {
                                let mut map = pending.lock().await;
                                if map.remove(&id).is_some() {
                                    // opcional: log de confirmação local
                                    // println!("💬 Mensagem {} confirmada: {}", id, info);
                                }
                            }
                        }
                        ChatMessage::ListResponse { users } => {
                            println!("\nUsuários online ({}):", users.len());
                            for u in users {
                                println!(" - {}", u.green());
                            }
                        }
                        ChatMessage::HistoryResponse { messages } => {
                            if messages.is_empty() {
                                println!("\n{}", "📜 Nenhuma mensagem no histórico".yellow());
                            } else {
                                println!("\n{} ({} mensagens):", "📜 Histórico".cyan().bold(), messages.len());
                                for msg in messages {
                                    // Formatar timestamp para exibição local
                                    let timestamp_display = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&msg.timestamp) {
                                        dt.with_timezone(&chrono::Local).format("%d/%m %H:%M:%S").to_string()
                                    } else {
                                        msg.timestamp.clone()
                                    };

                                    // Formatar baseado no tipo de mensagem
                                    match msg.message_type.as_str() {
                                        "private" => {
                                            let from = msg.from_user.cyan();
                                            let to = msg.to_user.as_ref().unwrap_or(&"?".to_string()).magenta();
                                            println!("  [{}] {} -> {}: {}",
                                                timestamp_display.dimmed(),
                                                from,
                                                to,
                                                msg.content
                                            );
                                        }
                                        "broadcast" => {
                                            let from = msg.from_user.green();
                                            println!("  [{}] {} (broadcast): {}",
                                                timestamp_display.dimmed(),
                                                from,
                                                msg.content
                                            );
                                        }
                                        "room" => {
                                            let from = msg.from_user.yellow();
                                            let room = msg.room.as_ref().unwrap_or(&"?".to_string()).blue();
                                            println!("  [{}] {} em #{}: {}",
                                                timestamp_display.dimmed(),
                                                from,
                                                room,
                                                msg.content
                                            );
                                        }
                                        _ => {
                                            println!("  [{}] {}: {}",
                                                timestamp_display.dimmed(),
                                                msg.from_user,
                                                msg.content
                                            );
                                        }
                                    }
                                }
                                println!(); // Linha em branco após histórico
                            }
                        }
                        ChatMessage::Error(e) => {
                            eprintln!("{} {}", "❌ Erro:".red().bold(), e);
                            // Sugere registrar caso o usuário não exista
                            if e.contains("não existe") || e.contains("não encontrado") {
                                println!("Se você quiser criar a conta, digite /register (usará o mesmo usuário/senha informados)");
                            }
                        }
                        ChatMessage::SessionToken { token, username: _ } => {
                            session_token = Some(token.clone());
                            println!("{} Autenticado com sucesso. Token recebido.", "✅".green());
                        }
                        ChatMessage::Typing { user, typing } => {
                            // Exibir typing indicator
                            if typing {
                                println!("{} {} está digitando...", "⌨️".cyan(), user.cyan().bold());
                            } else {
                                // Opcional: limpar indicador (por enquanto só não mostra nada)
                            }
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    error!("❌ JSON inválido recebido do servidor: {} - linha: {}", e, server_line.trim());
                    debug!("Raw line: {:?}", server_line);
                }
            }
                server_line.clear();
            }

            // Ouvindo o Teclado
            res = stdin.read_line(&mut user_line) => {
                res?;
                let content = user_line.trim();
                    if !content.is_empty() {
                    // Enviar typing indicator (apenas na primeira tecla)
                    {
                        let mut typing = is_typing.lock().await;
                        if !*typing {
                            *typing = true;
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
                        // Atualizar timestamp do último typing
                        let mut last_sent = last_typing_sent.lock().await;
                        *last_sent = Some(Instant::now());
                    }

                    if content.starts_with("/msg ") {
                        // Comando privado: /msg <user> <message>
                        let parts: Vec<&str> = content.splitn(3, ' ').collect();
                        if parts.len() == 3 {
                            let generated_id = Uuid::new_v4().to_string();
                            let msg = if let Some(token) = &session_token {
                                ChatMessage::Authenticated { token: token.clone(), action: Box::new(ChatAction::Private {
                                    from: username.clone(),
                                    to: parts[1].to_string(),
                                    content: parts[2].to_string(),
                                    timestamp: Utc::now(),
                                    message_id: Some(generated_id.clone()),
                                }) }
                            } else {
                                ChatMessage::Private {
                                    from: username.clone(),
                                    to: parts[1].to_string(),
                                    content: parts[2].to_string(),
                                    timestamp: Utc::now(),
                                    message_id: Some(generated_id.clone()),
                                }
                            };
                            let json = match serde_json::to_string(&msg) {
                                Ok(j) => j,
                                Err(e) => {
                                    error!("❌ Falha ao serializar DM: {}", e);
                                    user_line.clear();
                                    continue;
                                }
                            };
                            debug!(to=%parts[1], message_id=%generated_id, "Enviando DM");
                            // envia
                            {
                                let mut w = writer.lock().await;
                                if let Err(e) = w.write_all(format!("{}\n", json).as_bytes()).await {
                                    error!("❌ Falha ao enviar DM pela rede: {}", e);
                                    user_line.clear();
                                    continue;
                                }
                            }
                            // registra como pendente
                            {
                                let mut map = pending.lock().await;
                                map.insert(generated_id.clone(), (msg.clone(), Instant::now(), 1u8));
                            }
                            // Mostrar confirmação para o remetente
                            println!("{} {}: {}", "-> Você enviou para".blue(), parts[1].bold(), parts[2]);
                        }
                        } else if content == "/list" {
                        // Solicita lista de usuários online
                        let msg = if let Some(token) = &session_token {
                            ChatMessage::Authenticated { token: token.clone(), action: Box::new(ChatAction::ListRequest { from: username.clone() }) }
                        } else {
                            ChatMessage::ListRequest { from: username.clone() }
                        };
                        let json = serde_json::to_string(&msg)?;
                        {
                            let mut w = writer.lock().await;
                            w.write_all(format!("{}\n", json).as_bytes()).await?;
                        }
                        } else if content.starts_with("/history") {
                            // /history [username] [limit]
                            // Exemplos: /history, /history alice, /history alice 20
                            let parts: Vec<&str> = content.split_whitespace().collect();
                            let to_user = if parts.len() >= 2 { Some(parts[1].to_string()) } else { None };
                            let limit = if parts.len() >= 3 { parts[2].parse::<usize>().ok() } else { None };

                            let msg = if let Some(token) = &session_token {
                                ChatMessage::Authenticated {
                                    token: token.clone(),
                                    action: Box::new(ChatAction::HistoryRequest {
                                        from: username.clone(),
                                        to: to_user.clone(),
                                        limit,
                                    })
                                }
                            } else {
                                // Sem sessão, não pode buscar histórico
                                println!("{}", "❌ Você precisa estar autenticado para buscar histórico".red());
                                continue;
                            };

                            let json = serde_json::to_string(&msg)?;
                            {
                                let mut w = writer.lock().await;
                                w.write_all(format!("{}\n", json).as_bytes()).await?;
                            }

                            if let Some(to) = to_user {
                                println!("{} {} (limite: {})", "🔍 Buscando histórico com".cyan(), to.bold(), limit.unwrap_or(50));
                            } else {
                                println!("{} (limite: {})", "🔍 Buscando todo o histórico".cyan(), limit.unwrap_or(50));
                            }
                        } else if content.starts_with("/export") {
                            // /export <path>
                            let parts: Vec<&str> = content.splitn(2, ' ').collect();
                                if parts.len() >= 2 {
                                    // Suporta: `/export <path>` ou `/export --dm <path>` ou `/export --text <path>`
                                    let tokens: Vec<&str> = content.split_whitespace().collect();
                                    let (filter, path_opt) = match tokens.as_slice() {
                                        ["/export", flag, path] => {
                                            if *flag == "--dm" { (Some("dm"), Some(*path)) }
                                            else if *flag == "--text" { (Some("text"), Some(*path)) }
                                            else { (None, None) }
                                        }
                                        ["/export", path] => (None, Some(*path)),
                                        _ => (None, None),
                                    };

                                    if let Some(path) = path_opt {
                                        // Construir vetor filtrado
                                        let filtered: Vec<ChatMessage> = {
                                            let h = history.lock().await;
                                            h.iter().filter(|m| {
                                                match filter {
                                                    Some("dm") => matches!(m, ChatMessage::Private { .. }),
                                                    Some("text") => matches!(m, ChatMessage::Text { .. }),
                                                    _ => true,
                                                }
                                            }).cloned().collect()
                                        };

                                        debug!("Exportando {} mensagens para {}", filtered.len(), path);
                                        match serde_json::to_string_pretty(&filtered) {
                                            Ok(json) => match fs::write(path, json) {
                                                Ok(_) => {
                                                    info!("✅ Histórico exportado para {} ({} mensagens)", path, filtered.len());
                                                    println!("✅ Histórico exportado para {} ({} mensagens)", path, filtered.len());
                                                }
                                                Err(e) => {
                                                    error!("❌ Falha ao escrever arquivo {}: {}", path, e);
                                                    eprintln!("❌ Falha ao escrever {}: {}", path, e);
                                                }
                                            },
                                            Err(e) => {
                                                error!("❌ Falha ao serializar histórico para JSON: {}", e);
                                                eprintln!("❌ Falha ao serializar histórico: {}", e);
                                            }
                                        }
                                    } else {
                                        println!("Uso: /export [--dm|--text] <caminho_do_arquivo>");
                                    }
                                } else {
                                    println!("Uso: /export [--dm|--text] <caminho_do_arquivo>");
                                }
                        } else if content == "/history" {
                            // Mostrar histórico ordenado por timestamp
                            let h = history.lock().await;
                            let mut sorted: Vec<ChatMessage> = h.clone();
                            sorted.sort_by(|a, b| {
                                let ta = match a {
                                    ChatMessage::Text { timestamp, .. } => *timestamp,
                                    ChatMessage::Private { timestamp, .. } => *timestamp,
                                    _ => Utc::now(),
                                };
                                let tb = match b {
                                    ChatMessage::Text { timestamp, .. } => *timestamp,
                                    ChatMessage::Private { timestamp, .. } => *timestamp,
                                    _ => Utc::now(),
                                };
                                ta.cmp(&tb)
                            });

                            println!("--- Histórico (últimas {} mensagens) ---", sorted.len());
                            for m in sorted {
                                match m {
                                    ChatMessage::Text { from, content, timestamp } => {
                                        let local_ts = timestamp.with_timezone(&Local);
                                        let time_str = local_ts.format("%H:%M").to_string();
                                        println!("[{}] {}: {}", time_str.dimmed(), from.green().bold(), content);
                                    }
                                    ChatMessage::Private { from, content, timestamp, .. } => {
                                        let local_ts = timestamp.with_timezone(&Local);
                                        let time_str = local_ts.format("%H:%M").to_string();
                                        println!("[{}] {} {} {}", time_str.dimmed(), "🤫 [DM de".magenta(), from.magenta().bold(), "]:".magenta());
                                        println!("  {}", content.italic().white());
                                    }
                                    _ => {}
                                }
                            }
                    } else if content == "/register" {
                        // Envia registro usando as credenciais informadas inicialmente
                        let reg = ChatMessage::Register { username: username.clone(), password: password.clone() };
                        let json = serde_json::to_string(&reg)?;
                        let mut w = writer.lock().await;
                        w.write_all(format!("{}\n", json).as_bytes()).await?;
                    } else {
                        // Envio de texto normal (Broadcast)
                        let msg = if let Some(token) = &session_token {
                            ChatMessage::Authenticated { token: token.clone(), action: Box::new(ChatAction::Text { from: username.clone(), content: content.to_string(), timestamp: Utc::now() }) }
                        } else {
                            ChatMessage::Text { from: username.clone(), content: content.to_string(), timestamp: Utc::now() }
                        };
                        let json = match serde_json::to_string(&msg) {
                            Ok(j) => j,
                            Err(e) => {
                                error!("❌ Falha ao serializar mensagem broadcast: {}", e);
                                user_line.clear();
                                continue;
                            }
                        };
                        debug!("Enviando broadcast: {}", content);
                        {
                            let mut w = writer.lock().await;
                            if let Err(e) = w.write_all(format!("{}\n", json).as_bytes()).await {
                                error!("❌ Falha ao enviar broadcast pela rede: {}", e);
                                user_line.clear();
                                continue;
                            }
                        }
                    }
                }
                user_line.clear();
            }
        }
    }

    warn!("⚠️ Loop principal encerrado - conexão com servidor perdida");
    println!("Conexão encerrada.");
    Ok(())
}
