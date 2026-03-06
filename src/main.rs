use chat_serve::client;
use chat_serve::server::auth::AuthManager;
use chat_serve::server::core::ChatCore;
use chat_serve::server::database::Database;
use chat_serve::server::listener;
use chat_serve::server::state::ChatState;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Inicializamos o Estado Global envolto em um Arc (Atomic Reference Counter)
    // O Arc permite que o Estado seja compartilhado entre várias "tasks" do Tokio
    // Inicializa o subscriber de tracing usando `RUST_LOG` para filtro.
    // Se a variável de ambiente `LOG_JSON` estiver presente (1/true/yes),
    // usamos saída em JSON para compatibilidade com pipelines (ELK/Loki).
    let env_filter = tracing_subscriber::EnvFilter::from_default_env();
    let log_json = std::env::var("LOG_JSON")
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    if log_json {
        tracing_subscriber::fmt()
            .event_format(tracing_subscriber::fmt::format().json())
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    }

    let state = Arc::new(ChatState::new());
    // Inicializa salas fixas (#geral, #ajuda, #dev)
    state.init_fixed_rooms();

    // 2. Criamos o Core do Servidor passando o estado
    // Inicializa o AuthManager com caminho de arquivo configurável
    let users_file = std::env::var("USERS_FILE").unwrap_or_else(|_| "users.json".to_string());
    let auth = Arc::new(AuthManager::new(&users_file));

    // Inicializa o Database (usa o mesmo caminho base do AuthManager)
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| users_file.replace(".json", ".db"));
    let db = Arc::new(Database::new(&db_path).expect("Falha ao inicializar banco de dados"));

    // Task de limpeza automática de mensagens antigas
    {
        let db_clone = db.clone();
        let retention_days: u64 = std::env::var("MESSAGE_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(90);

        let pending_ttl_days: Option<u64> = std::env::var("PENDING_MESSAGE_TTL_DAYS")
            .ok()
            .and_then(|v| v.parse().ok());

        if let Some(ttl) = pending_ttl_days {
            info!(retention_days=%retention_days, pending_ttl_days=%ttl, "🗑️  Limpeza automática de mensagens configurada (com TTL para pendentes)");
        } else {
            info!(retention_days=%retention_days, "🗑️  Limpeza automática de mensagens configurada");
        }

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(24 * 3600)); // 24 horas
            loop {
                interval.tick().await;
                let db_task = db_clone.clone();
                let days = retention_days;
                let pending_ttl = pending_ttl_days;

                tokio::task::spawn_blocking(move || {
                    // Limpeza geral de mensagens antigas
                    match db_task.delete_messages_older_than(days as i64) {
                        Ok(deleted) => {
                            if deleted > 0 {
                                info!(deleted=%deleted, days=%days, "🗑️  Mensagens antigas removidas");
                            }
                        }
                        Err(e) => {
                            error!(error=%e, days=%days, "Falha ao limpar mensagens antigas");
                        }
                    }

                    // Limpeza adicional de mensagens pendentes (TTL)
                    if let Some(ttl) = pending_ttl {
                        match db_task.delete_pending_messages_older_than(ttl as i64) {
                            Ok(deleted) => {
                                if deleted > 0 {
                                    info!(deleted=%deleted, ttl_days=%ttl, "🗑️  Mensagens pendentes antigas removidas (TTL)");
                                }
                            }
                            Err(e) => {
                                error!(error=%e, ttl_days=%ttl, "Falha ao limpar mensagens pendentes antigas");
                            }
                        }
                    }
                }).await.ok();
            }
        });
    }

    let core = Arc::new(ChatCore::new_with_auth_and_db(state.clone(), auth, db));

    // Background task: Atualizar métricas de gauge periodicamente
    {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                chat_serve::utils::metrics::set_users_online(state_clone.client_count() as i64);
                chat_serve::utils::metrics::set_active_rooms(state_clone.rooms.len() as i64);
            }
        });
    }

    // Start metrics server in background (Prometheus scrape endpoint)
    let metrics_bind =
        std::env::var("METRICS_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:9090".to_string());
    tokio::spawn(async move {
        if let Err(e) = chat_serve::utils::metrics_server::run_metrics_server(&metrics_bind).await {
            tracing::error!(error=%e, "metrics server failed");
        }
    });

    // 3. TLS: Carrega certificados se TLS_ENABLED=true
    let tls_enabled = std::env::var("TLS_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let tls_acceptor = if tls_enabled {
        let cert_path = std::env::var("TLS_CERT").unwrap_or_else(|_| "certs/cert.pem".to_string());
        let key_path = std::env::var("TLS_KEY").unwrap_or_else(|_| "certs/key.pem".to_string());

        match listener::load_tls_acceptor(&cert_path, &key_path) {
            Ok(acceptor) => {
                info!(cert=%cert_path, key=%key_path, "🔒 TLS habilitado");
                Some(acceptor)
            }
            Err(e) => {
                error!(error=%e, "Falha ao carregar certificados TLS, abortando");
                return Err(e.into());
            }
        }
    } else {
        info!("⚠️  TLS desabilitado (modo inseguro)");
        None
    };

    // 4. Abrimos o "Portão" (Socket TCP)
    // Bind address pode ser configurado via env var `BIND_ADDR` (default `0.0.0.0:8080`).
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let listener = TcpListener::bind(&bind_addr).await?;
    info!(bind_addr=%bind_addr, "🚀 Servidor de Chat rodando");

    // 5. Loop: Aceitar novas conexões com graceful shutdown
    loop {
        tokio::select! {
            // Aceita nova conexão
            result = listener.accept() => {
                match result {
                    Ok((socket, addr)) => {
                        // Clonamos o core e tls_acceptor para passar para a nova task
                        let core_handler = core.clone();
                        let tls_acceptor = tls_acceptor.clone();

                        let peer = chat_serve::utils::logger::peer_display(&addr, core_handler.username_by_addr(&addr).as_deref());
                        info!(peer=%peer, "✨ Nova conexão recebida; iniciando handler");

                        // 6. Spawn: Cria uma "thread leve" para cuidar desse cliente específico
                        tokio::spawn(async move {
                            if let Some(acceptor) = tls_acceptor {
                                // TLS handshake
                                match acceptor.accept(socket).await {
                                    Ok(tls_stream) => {
                                        client::handle::handle_connection(tls_stream, addr, core_handler.clone()).await;
                                    }
                                    Err(e) => {
                                        error!(peer=%peer, error=%e, "Falha no TLS handshake");
                                    }
                                }
                            } else {
                                // TCP puro (sem TLS)
                                client::handle::handle_connection(socket, addr, core_handler.clone()).await;
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!(error=%e, "Falha ao aceitar conexão");
                    }
                }
            }
            // Aguarda sinal de shutdown (SIGINT/SIGTERM)
            _ = shutdown_signal() => {
                warn!("🛑 Sinal de shutdown recebido, encerrando servidor gracefully...");
                break;
            }
        }
    }

    info!("👋 Servidor encerrado");
    Ok(())
}

/// Aguarda sinal de shutdown (SIGINT/SIGTERM)
/// No Unix: SIGINT (Ctrl+C) ou SIGTERM
/// No Windows: Ctrl+C
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
