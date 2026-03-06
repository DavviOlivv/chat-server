use tokio::io::{AsyncBufReadExt, AsyncWriteExt, AsyncRead, AsyncWrite, BufReader};
use std::net::SocketAddr;
use std::sync::Arc;
use crate::server::core::ChatCore;
use crate::model::message::ChatMessage;
use tokio::sync::mpsc;
use tracing::{info, debug, error};

pub async fn handle_connection<S>(stream: S, addr: SocketAddr, core: Arc<ChatCore>)
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // 1. Criamos um canal para este cliente específico receber mensagens
    let (tx, mut rx) = mpsc::channel::<ChatMessage>(1000);

    // 2. Dividimos o stream em Leitura e Escrita
    let (reader, writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let mut writer = writer;
    let mut line = String::new();

    info!("Log: Iniciando loop de conexão");
    // tenta resolver username conhecido para logs futuros
    let maybe_name = core.username_by_addr(&addr);

    // 3. O "Select" do Tokio: O coração da concorrência por conexão
    loop {
        tokio::select! {
            // Caso A: Chegou uma linha de texto do Cliente via rede
            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) => break, // Conexão fechada pelo cliente
                    Ok(_n) => {
                        let trimmed = line.trim(); // Remove \n, \r e espaços
                        if trimmed.is_empty() {
                            line.clear();
                            continue;
                        }

                        // DEBUG: Vamos imprimir exatamente o que chegou para ter certeza
                        let peer = crate::utils::logger::peer_display(&addr, maybe_name.as_deref());
                        debug!("Recebido de {}: [{}]", peer, trimmed);

                        match serde_json::from_str::<ChatMessage>(trimmed) {
                            Ok(msg) => {
                                debug!(peer=%peer, msg_type=%format!("{:?}", std::mem::discriminant(&msg)), "Mensagem decodificada com sucesso");
                                // Se for Login, delegamos para o core tratar autenticação (recebe tx)
                                if let ChatMessage::Login { ref username, ref password } = msg {
                                    core.handle_login(username.clone(), password.clone(), addr, tx.clone());
                                } else if let ChatMessage::Register { ref username, ref password } = msg {
                                    core.handle_register(username.clone(), password.clone(), addr, tx.clone());
                                } else {
                                    core.handle_message(msg, addr);
                                }
                            }
                            Err(e) => {
                                let peer = crate::utils::logger::peer_display(&addr, maybe_name.as_deref());
                                error!("❌ Erro ao decodificar JSON de {} - erro: {} - raw: [{}]", peer, e, trimmed);
                                // Opcional: Enviar um erro de volta para o cliente
                                let error_msg = ChatMessage::Error(format!("JSON inválido: {}", e));
                                if let Ok(json) = serde_json::to_string(&error_msg) {
                                    if let Err(write_err) = writer.write_all(format!("{}\n", json).as_bytes()).await {
                                        error!("Falha ao enviar mensagem de erro para cliente {}: {}", peer, write_err);
                                    }
                                }
                            }
                        }
                        line.clear();
                    }
                    Err(e) => {
                        let peer = crate::utils::logger::peer_display(&addr, maybe_name.as_deref());
                        error!("❌ Erro de I/O ao ler de {}: {}", peer, e);
                        break;
                    }
                }
            }

            // Caso B: O Servidor (Core) enviou uma mensagem para este cliente via Canal
            Some(msg) = rx.recv() => {
                match serde_json::to_string(&msg) {
                    Ok(json) => {
                        if let Err(e) = writer.write_all(format!("{}\n", json).as_bytes()).await {
                            let peer = crate::utils::logger::peer_display(&addr, maybe_name.as_deref());
                            error!("❌ Falha ao enviar mensagem para {}: {}", peer, e);
                            break;
                        } else {
                            debug!(peer=%addr, msg_type=%format!("{:?}", std::mem::discriminant(&msg)), "Mensagem enviada para cliente");
                        }
                    }
                    Err(e) => {
                        let peer = crate::utils::logger::peer_display(&addr, maybe_name.as_deref());
                        error!("❌ Falha ao serializar mensagem para {}: {}", peer, e);
                    }
                }
            }
        }
    }
    
    // Saindo do loop (desconexão), limpamos o rastro do usuário
    core.disconnect_user(addr);
    // Tenta resolver username para logs mais amigáveis
    let peer = crate::utils::logger::peer_display(&addr, core.username_by_addr(&addr).as_deref());
    info!("Log: Cliente {} desconectado.", peer);
}