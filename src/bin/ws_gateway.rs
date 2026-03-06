use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use futures_util::{StreamExt, SinkExt};
use tracing::{info, error, warn};

/// WebSocket Gateway - Faz bridge entre clientes WebSocket e servidor TCP
/// 
/// Arquitetura:
/// WebSocket Client → WS Gateway (8081) → TCP Server (8080)
///                         ↕
///                  JSON translation
///
/// O gateway mantém duas conexões:
/// 1. WebSocket com o cliente web
/// 2. TCP com o servidor chat principal
///
/// Traduz mensagens bidirecionalmente mantendo o protocolo JSON

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup de logging
    tracing_subscriber::fmt()
        .with_env_filter("ws_gateway=info,tokio_tungstenite=warn")
        .init();

    // Endereço do gateway WebSocket
    let ws_addr: SocketAddr = "127.0.0.1:8081".parse()?;
    let listener = TcpListener::bind(&ws_addr).await?;
    
    info!("🌐 WebSocket Gateway rodando em ws://{}", ws_addr);
    info!("📡 Fazendo bridge para servidor TCP em 127.0.0.1:8080");
    info!("💡 Clientes web podem conectar via WebSocket");

    // Loop de aceitação de conexões WebSocket
    while let Ok((stream, peer_addr)) = listener.accept().await {
        info!("Nova conexão WebSocket de {}", peer_addr);
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, peer_addr).await {
                error!("Erro ao processar conexão {}: {}", peer_addr, e);
            }
        });
    }

    Ok(())
}

async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
) -> Result<(), Box<dyn std::error::Error>> {
    // Upgrade para WebSocket
    let ws_stream = accept_async(stream).await?;
    info!("WebSocket handshake completo para {}", peer_addr);

    // Conectar ao servidor TCP principal
    let tcp_stream = TcpStream::connect("127.0.0.1:8080").await?;
    info!("Conectado ao servidor TCP para {}", peer_addr);

    let (mut ws_write, mut ws_read) = ws_stream.split();
    let (tcp_read, mut tcp_write) = tcp_stream.into_split();
    let mut tcp_reader = BufReader::new(tcp_read);

    // Task 1: WebSocket → TCP (mensagens do cliente web para servidor)
    let ws_to_tcp = tokio::spawn(async move {
        while let Some(msg) = ws_read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    // Cliente web envia JSON como texto WebSocket
                    info!("WS→TCP [{}]: {}", peer_addr, text.chars().take(100).collect::<String>());
                    
                    // Encaminhar para servidor TCP (adiciona newline pois servidor espera linhas)
                    if let Err(e) = tcp_write.write_all(text.as_bytes()).await {
                        error!("Erro ao escrever no TCP: {}", e);
                        break;
                    }
                    if let Err(e) = tcp_write.write_all(b"\n").await {
                        error!("Erro ao escrever newline no TCP: {}", e);
                        break;
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Cliente {} fechou conexão WebSocket", peer_addr);
                    break;
                }
                Ok(Message::Ping(_data)) => {
                    // WebSocket ping/pong automático
                    info!("Ping recebido de {}", peer_addr);
                }
                Ok(Message::Pong(_)) => {
                    // Resposta a ping
                }
                Ok(Message::Binary(_)) => {
                    warn!("Mensagem binária ignorada de {} (esperado texto JSON)", peer_addr);
                }
                Err(e) => {
                    error!("Erro ao ler WebSocket de {}: {}", peer_addr, e);
                    break;
                }
                _ => {}
            }
        }
        info!("Task WS→TCP encerrada para {}", peer_addr);
    });

    // Task 2: TCP → WebSocket (mensagens do servidor para cliente web)
    let tcp_to_ws = tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match tcp_reader.read_line(&mut line).await {
                Ok(0) => {
                    info!("Servidor TCP fechou conexão para {}", peer_addr);
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        info!("TCP→WS [{}]: {}", peer_addr, trimmed.chars().take(100).collect::<String>());
                        
                        // Encaminhar para cliente WebSocket como texto
                        if let Err(e) = ws_write.send(Message::Text(trimmed.to_string())).await {
                            error!("Erro ao escrever no WebSocket: {}", e);
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Erro ao ler do TCP: {}", e);
                    break;
                }
            }
        }
        info!("Task TCP→WS encerrada para {}", peer_addr);
        
        // Fechar WebSocket graciosamente
        let _ = ws_write.close().await;
    });

    // Aguardar ambas as tasks
    let _ = tokio::try_join!(ws_to_tcp, tcp_to_ws);
    
    info!("Conexão {} totalmente encerrada", peer_addr);
    Ok(())
}
