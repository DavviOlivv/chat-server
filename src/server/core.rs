use std::net::SocketAddr;
use crate::model::message::{ChatMessage, AckKind, ChatAction, StoredMessage};
use crate::server::auth::AuthManager;
use crate::server::database::{Database, MessageType};
use chrono::{DateTime, Utc};
use crate::server::state::ChatState;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn, error, debug};
use tokio::sync::mpsc::error::TrySendError;
use crate::utils::metrics;

type Tx = mpsc::Sender<ChatMessage>;

// Limite básico de mensagens por usuário (mensagens por segundo).
// Pode ser configurado via env var `RATE_LIMIT_PER_SEC` (default: 10).
fn rate_limit_per_sec() -> u32 {
    std::env::var("RATE_LIMIT_PER_SEC")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10)
}

pub struct ChatCore {
    state: Arc<ChatState>,
    auth: Option<Arc<AuthManager>>,
    db: Option<Arc<Database>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::state::ChatState;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use chrono::Utc;
    use std::net::SocketAddr;

    #[test]
    fn test_send_private_and_ack() {
        let state = Arc::new(ChatState::new());
        let (tx_a, mut rx_a) = mpsc::channel(1000);
        let (tx_b, mut rx_b) = mpsc::channel(1000);

        let addr_a: SocketAddr = "127.0.0.1:10000".parse().unwrap();
        let addr_b: SocketAddr = "127.0.0.1:10001".parse().unwrap();

        state.add_client(addr_a, "alice".to_string(), tx_a);
        state.add_client(addr_b, "bob".to_string(), tx_b);

        let core = ChatCore::new(state.clone());

        core.send_private_message("alice".into(), "bob".into(), "hello".into(), Utc::now(), Some("mid-1".into()));

        // bob should receive Private
        match rx_b.try_recv() {
            Ok(msg) => match msg {
                ChatMessage::Private { from, to, content, message_id, .. } => {
                    assert_eq!(from, "alice");
                    assert_eq!(to, "bob");
                    assert_eq!(content, "hello");
                    assert_eq!(message_id, Some("mid-1".into()));
                }
                _ => panic!("expected Private"),
            },
            Err(e) => panic!("bob did not receive private: {:?}", e),
        }

        // alice should receive Ack
        match rx_a.try_recv() {
            Ok(msg) => match msg {
                ChatMessage::Ack { kind, message_id, .. } => {
                    assert_eq!(kind, AckKind::Delivered);
                    assert_eq!(message_id, Some("mid-1".into()));
                }
                _ => panic!("expected Ack"),
            },
            Err(e) => panic!("alice did not receive ack: {:?}", e),
        }
    }

    #[test]
    fn test_dedup() {
        let state = Arc::new(ChatState::new());
        assert_eq!(state.check_and_mark_message_id("dup-1"), false);
        assert_eq!(state.check_and_mark_message_id("dup-1"), true);
    }

    #[test]
    fn test_roomtext_barrier_and_broadcast() {
        let state = Arc::new(ChatState::new());
        state.init_fixed_rooms();

        let (tx_a, mut rx_a) = mpsc::channel(1000);
        let (tx_b, mut rx_b) = mpsc::channel(1000);

        let addr_a: SocketAddr = "127.0.0.1:11000".parse().unwrap();
        let addr_b: SocketAddr = "127.0.0.1:11001".parse().unwrap();

        state.add_client(addr_a, "alice".to_string(), tx_a);
        state.add_client(addr_b, "bob".to_string(), tx_b);

        let core = ChatCore::new(state.clone());

        // bob joins #rust, alice does not
        state.join_room("bob", "#rust");

        // alice attempts to send RoomText to #rust (should be denied)
        core.handle_message(ChatMessage::RoomText { from: "alice".into(), room: "#rust".into(), content: "hi".into(), timestamp: Utc::now() }, addr_a);

        // alice should receive Ack::Failed
        match rx_a.try_recv() {
            Ok(msg) => match msg {
                ChatMessage::Ack { kind, .. } => {
                    assert_eq!(kind, AckKind::Failed);
                }
                _ => panic!("expected Ack::Failed"),
            },
            Err(e) => panic!("alice did not receive ack: {:?}", e),
        }

        // bob should NOT receive the message
        match rx_b.try_recv() {
            Ok(_) => panic!("bob should not receive RoomText"),
            Err(_) => {}
        }

        // Now alice joins and sends again
        state.join_room("alice", "#rust");
        core.handle_message(ChatMessage::RoomText { from: "alice".into(), room: "#rust".into(), content: "hi2".into(), timestamp: Utc::now() }, addr_a);

        // bob should receive the RoomText
        match rx_b.try_recv() {
            Ok(msg) => match msg {
                ChatMessage::RoomText { from, room, content, .. } => {
                    assert_eq!(from, "alice");
                    assert_eq!(room, "#rust");
                    assert_eq!(content, "hi2");
                }
                _ => panic!("expected RoomText"),
            },
            Err(e) => panic!("bob did not receive roomtext: {:?}", e),
        }
    }
}

impl ChatCore {
    pub fn new(state: Arc<ChatState>) -> Self {
        Self { state, auth: None, db: None }
    }

    pub fn new_with_auth(state: Arc<ChatState>, auth: Arc<AuthManager>) -> Self {
        Self { state, auth: Some(auth), db: None }
    }

    pub fn new_with_auth_and_db(state: Arc<ChatState>, auth: Arc<AuthManager>, db: Arc<Database>) -> Self {
        Self { state, auth: Some(auth), db: Some(db) }
    }

    /// Envia de forma confiável quando possível: se houver um runtime Tokio ativo,
    /// spawnamos uma task que `await`s o `send`. Caso contrário (ex.: testes
    /// síncronos), tentamos `try_send` como fallback.
    fn send_reliable(&self, tx: mpsc::Sender<ChatMessage>, msg: ChatMessage) {
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move {
                let _ = tx.send(msg).await;
            });
        } else {
            let _ = tx.try_send(msg);
        }
    }

    /// Retorna o username associado a um `SocketAddr`, se conhecido.
    pub fn username_by_addr(&self, addr: &SocketAddr) -> Option<String> {
        self.state.username_by_addr(addr)
    }

    pub fn handle_message(&self, msg: ChatMessage, addr: SocketAddr) {
        match msg {
            ChatMessage::Login { username, .. } => {
                warn!("Tentativa de login via handle_message para {}. Use register_user.", username);
            }
            ChatMessage::Register { .. } => {
                warn!("Mensagem Register recebida em handle_message. Autenticação não implementada neste fluxo.");
            }
            ChatMessage::SessionToken { .. } => {
                warn!("Mensagem SessionToken recebida em handle_message. Ignorada.");
            }
            ChatMessage::Authenticated { token, action } => {
                // Se não tivermos um AuthManager configurado, recusamos
                let auth = match &self.auth {
                    Some(a) => a,
                    None => {
                        warn!("Received Authenticated message but AuthManager not configured");
                        return;
                    }
                };

                if let Some(username) = auth.validate_token(&token) {
                    // Processa a ação autenticada usando o username derivado do token
                    self.handle_authenticated_action(username, *action, addr);
                } else {
                    warn!("Token inválido recebido de {}", addr);
                    // Tenta informar o cliente (se já estiver registrado por addr)
                    if let Some(username) = self.username_by_addr(&addr) {
                        if let Some(tx) = self.state.get_client_tx(&username) {
                            self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: "Token inválido ou expirado".to_string(), message_id: None });
                        }
                    }
                }
            }
            ChatMessage::JoinRoom { room } => {
                // Resolve username pelo endereco
                if let Some(username) = self.username_by_addr(&addr) {
                    // Rate limiting por usuário
                    if !self.state.check_rate_limit(&username, rate_limit_per_sec()) {
                        if let Some(tx) = self.state.get_client_tx(&username) {
                            self.send_reliable(tx, ChatMessage::Ack {
                                kind: AckKind::Failed,
                                info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()),
                                message_id: None,
                            });
                        }
                        return;
                    }
                    self.state.join_room(&username, &room);
                    info!(user=%username, room=%room, action="join");
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::System, info: format!("Entrou na sala {}", room), message_id: None });
                    }
                } else {
                    warn!("JoinRoom pedido de endereco desconhecido: {}", addr);
                }
            }
            ChatMessage::LeaveRoom { room } => {
                if let Some(username) = self.username_by_addr(&addr) {
                    if !self.state.check_rate_limit(&username, rate_limit_per_sec()) {
                        if let Some(tx) = self.state.get_client_tx(&username) {
                            self.send_reliable(tx, ChatMessage::Ack {
                                kind: AckKind::Failed,
                                info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()),
                                message_id: None,
                            });
                        }
                        return;
                    }
                    self.state.leave_room(&username, &room);
                    info!(user=%username, room=%room, action="leave");
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::System, info: format!("Saiu da sala {}", room), message_id: None });
                    }
                } else {
                    warn!("LeaveRoom pedido de endereco desconhecido: {}", addr);
                }
            }
            ChatMessage::Text { from, content, timestamp } => {
                // Rate limiting usando username associado ao endereço (fallback para `from`)
                let key = self.username_by_addr(&addr).unwrap_or_else(|| from.clone());
                if !self.state.check_rate_limit(&key, rate_limit_per_sec()) {
                    if let Some(tx) = self.state.get_client_tx(&key) {
                        self.send_reliable(tx, ChatMessage::Ack {
                            kind: AckKind::Failed,
                            info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()),
                            message_id: None,
                        });
                    }
                    return;
                }
                self.broadcast(from, content, timestamp);
            }
            ChatMessage::RoomText { from: _, room, content, timestamp } => {
                // quem enviou é consultado via endereco
                if let Some(username) = self.username_by_addr(&addr) {
                    if !self.state.check_rate_limit(&username, rate_limit_per_sec()) {
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx.clone(), ChatMessage::Ack {
                                    kind: AckKind::Failed,
                                    info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()),
                                    message_id: None,
                                });
                            }
                        return;
                    }
                    // Verifica inscrição no mapa B
                    let in_room = self.state.subscriptions.get(&username).map_or(false, |s| s.contains(&room));
                    if in_room {
                        // Envia para todos os membros daquela sala
                        if let Some(members) = self.state.rooms.get(&room) {
                            let msg = ChatMessage::RoomText { from: username.clone(), room: room.clone(), content: content.clone(), timestamp };
                            for member in members.iter() {
                                let member_name = member.key();
                                if let Some(tx) = self.state.get_client_tx(member_name) {
                                            if let Err(e) = tx.try_send(msg.clone()) {
                                                            match e {
                                                                TrySendError::Full(_) => metrics::inc_send_full(),
                                                                _ => {}
                                                            }
                                                            error!(to=%member_name, error=%e, "Falha ao enviar RoomText");
                                                        }
                                }
                            }
                        }
                    } else {
                        // Envia Ack de falha para o remetente
                        if let Some(tx) = self.state.get_client_tx(&username) {
                            self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: format!("Você precisa entrar na sala {} para enviar mensagens.", room), message_id: None });
                        }
                    }
                } else {
                    warn!("RoomText pedido de endereco desconhecido: {}", addr);
                }
            }
            ChatMessage::Private { from, to, content, timestamp, message_id } => {
                // Rate limiting baseado no remetente lógico
                if !self.state.check_rate_limit(&from, rate_limit_per_sec()) {
                    if let Some(tx) = self.state.get_client_tx(&from) {
                        self.send_reliable(tx, ChatMessage::Ack {
                            kind: AckKind::Failed,
                            info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()),
                            message_id: message_id.clone(),
                        });
                    }
                    return;
                }
                self.send_private_message(from, to, content, timestamp, message_id);
            }
            ChatMessage::ListRequest { from } => {
                if !self.state.check_rate_limit(&from, rate_limit_per_sec()) {
                    if let Some(tx) = self.state.get_client_tx(&from) {
                        self.send_reliable(tx, ChatMessage::Ack {
                            kind: AckKind::Failed,
                            info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()),
                            message_id: None,
                        });
                    }
                    return;
                }
                // Gera lista de usernames e envia de volta ao solicitante
                let users = self.state.list_usernames();
                if let Some(sender_tx) = self.state.get_client_tx(&from) {
                    self.send_reliable(sender_tx, ChatMessage::ListResponse { users });
                }
            }
            ChatMessage::ReadReceipt { message_id, reader } => {
                // Read receipt: notificar o remetente original
                info!(reader=%reader, message_id=%message_id, "📖 Read receipt recebido");
                
                // Broadcast para todos os usuários online
                // (em produção real, busque no DB quem enviou o message_id)
                for entry in self.state.clients.iter() {
                    let target_username = entry.key();
                    if target_username != &reader {
                        let tx = entry.value().1.clone();
                        let ack = ChatMessage::Ack {
                            kind: AckKind::Read,
                            info: format!("{} leu sua mensagem", reader),
                            message_id: Some(message_id.clone()),
                        };
                        let _ = tx.try_send(ack);
                    }
                }
            }
            ChatMessage::Typing { user, typing } => {
                // Typing indicator: broadcast para todos
                debug!(user=%user, typing=%typing, "⌨️ Typing indicator");
                
                let typing_msg = ChatMessage::Typing {
                    user: user.clone(),
                    typing,
                };
                
                for entry in self.state.clients.iter() {
                    let target_username = entry.key();
                    if target_username != &user {
                        let tx = entry.value().1.clone();
                        let _ = tx.try_send(typing_msg.clone());
                    }
                }
            }
            _ => {} 
        }
    }

    pub fn send_private_message(&self, from: String, to: String, content: String, timestamp: DateTime<Utc>, message_id: Option<String>) {
        // Métricas: mensagem privada enviada
        metrics::inc_messages_sent();
        metrics::inc_private_messages();
        
        // 1. Tenta encontrar o destinatário
        // Se a mensagem possui um message_id, verifique duplicata primeiro
        if let Some(ref id) = message_id {
            if self.state.check_and_mark_message_id(id) {
                // Já processada antes: não reenviamos. Informamos o remetente que já foi entregue.
                if let Some(sender_tx) = self.state.get_client_tx(&from) {
                    self.send_reliable(sender_tx, ChatMessage::Ack {
                        kind: AckKind::Delivered,
                        info: format!("DM para {} já entregue (dedup).", to),
                        message_id: Some(id.clone()),
                    });
                }
                return;
            }
        }

        // Verifica se destinatário está online
        let is_recipient_online = self.state.get_client_tx(&to).is_some();
        
        // Se não está online, verificar se o usuário existe
        let recipient_exists = if !is_recipient_online {
            if let Some(auth) = &self.auth {
                // Com AuthManager, verifica se usuário existe no banco de dados
                auth.user_exists(&to)
            } else {
                // Sem AuthManager, verifica se está na lista de clientes conhecidos
                // (usuários que já se conectaram alguma vez nesta sessão)
                self.state.has_user(&to)
            }
        } else {
            true // Está online, logo existe
        };

        // Se usuário não existe, retorna erro
        if !recipient_exists {
            if let Some(sender_tx) = self.state.get_client_tx(&from) {
                self.send_reliable(sender_tx, ChatMessage::Error(format!("Usuário '{}' não encontrado.", to)));
            }
            return;
        }

        // Se destinatário está offline, verifica limite de mensagens pendentes
        if !is_recipient_online {
            if let Some(db) = &self.db {
                // Limite configurável via env var (padrão: 1000)
                let max_pending = std::env::var("MAX_PENDING_PER_USER")
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(1000);
                
                // Conta mensagens pendentes do destinatário
                match db.count_pending_messages(&to) {
                    Ok(count) if count >= max_pending => {
                        // Métricas: fila cheia
                        metrics::inc_queue_full();
                        warn!(from=%from, to=%to, pending=%count, max=%max_pending, "Caixa de entrada cheia");
                        if let Some(sender_tx) = self.state.get_client_tx(&from) {
                            self.send_reliable(sender_tx, ChatMessage::Error(
                                format!("Caixa de entrada de '{}' cheia ({}/{} mensagens). Tente mais tarde.", to, count, max_pending)
                            ));
                        }
                        return;
                    }
                    Err(e) => {
                        debug!(error=%e, "Falha ao verificar limite de mensagens pendentes");
                        // Continua mesmo com erro na verificação (fail-open)
                    }
                    _ => {} // OK, abaixo do limite
                }
            }
        }

        // Salva mensagem no banco de dados com status de entrega correto
        if let Some(db) = &self.db {
            let db_clone = db.clone();
            let from_clone = from.clone();
            let to_clone = to.clone();
            let content_clone = content.clone();
            let timestamp_clone = timestamp.clone();
            let message_id_clone = message_id.clone();
            let delivered = is_recipient_online;
            
            tokio::task::spawn_blocking(move || {
                if let Err(e) = db_clone.insert_message_with_delivery(
                    &from_clone,
                    Some(&to_clone),
                    None,
                    &content_clone,
                    &timestamp_clone,
                    message_id_clone.as_deref(),
                    MessageType::Private,
                    delivered,
                ) {
                    debug!(error=%e, "Falha ao salvar DM no banco");
                }
            });
        }

        if is_recipient_online {
            let target_tx = self.state.get_client_tx(&to).unwrap();
            let msg = ChatMessage::Private {
                from: from.clone(),
                to: to.clone(),
                content: content.clone(),
                timestamp,
                message_id: message_id.clone(),
            };

            // 2. Envia para o destinatário — preferimos spawnar uma task
            //    e aguardar o `send().await` quando houver runtime; caso
            //    contrário (ex.: testes síncronos), usamos `try_send`.
            let target_tx_clone = target_tx.clone();
            let sender_tx_opt = self.state.get_client_tx(&from);
            let to_clone = to.clone();
            let message_id_clone = message_id.clone();

            if tokio::runtime::Handle::try_current().is_ok() {
                tokio::spawn(async move {
                    match target_tx_clone.send(msg).await {
                            Ok(()) => {
                                // Métricas: mensagem entregue imediatamente
                                metrics::inc_messages_delivered();
                                if let Some(sender_tx) = sender_tx_opt {
                                    let _ = sender_tx.send(ChatMessage::Ack {
                                        kind: AckKind::Delivered,
                                        info: format!("DM para {} entregue.", to_clone),
                                        message_id: message_id_clone.clone(),
                                    }).await;
                                }
                            }
                            Err(e) => {
                                // on async send error (closed) increment metric if appropriate
                                error!(to=%to_clone, error=%e, "Falha ao enviar DM");
                                if let Some(sender_tx) = sender_tx_opt {
                                    let _ = sender_tx.send(ChatMessage::Error(format!("Falha ao entregar DM para {}.", to_clone))).await;
                                }
                            }
                        }
                });
            } else {
                // No runtime (tests), fallback para try_send síncrono
                if let Err(e) = target_tx.try_send(msg) {
                    match e {
                        TrySendError::Full(_) => metrics::inc_send_full(),
                        _ => {}
                    }
                    error!(to=%to_clone, error=%e, "Falha ao enviar DM (fallback try_send)");
                    if let Some(sender_tx) = sender_tx_opt {
                        self.send_reliable(sender_tx, ChatMessage::Error(format!("Falha ao entregar DM para {}.", to_clone)));
                    }
                } else {
                    if let Some(sender_tx) = sender_tx_opt {
                        self.send_reliable(sender_tx, ChatMessage::Ack {
                            kind: AckKind::Delivered,
                            info: format!("DM para {} entregue.", to_clone),
                            message_id: message_id_clone.clone(),
                        });
                    }
                }
            }
        } else {
            // 3. Destinatário está OFFLINE - mensagem já salva com delivered=0
            // Métricas: mensagem em fila para entrega offline
            metrics::inc_messages_queued();
            info!(from=%from, to=%to, "📭 Mensagem salva para entrega offline");
            
            // Avisa o remetente que a mensagem será entregue quando o destinatário voltar online
            if let Some(sender_tx) = self.state.get_client_tx(&from) {
                self.send_reliable(sender_tx, ChatMessage::Ack {
                    kind: AckKind::Received,
                    info: format!("Mensagem para {} será entregue quando voltar online.", to),
                    message_id: message_id.clone(),
                });
            }
        }
    }

    fn handle_authenticated_action(&self, username: String, action: ChatAction, _addr: SocketAddr) {
        // Verificar se usuário está silenciado (exceto para comandos admin)
        let is_admin_action = matches!(action, 
            ChatAction::AdminKick { .. } | 
            ChatAction::AdminBan { .. } |
            ChatAction::AdminMute { .. } |
            ChatAction::AdminUnmute { .. } |
            ChatAction::AdminPromote { .. } |
            ChatAction::AdminDemote { .. } |
            ChatAction::AdminList |
            ChatAction::AdminLogs { .. } |
            ChatAction::SearchMessages { .. } |
            ChatAction::SendFile { .. }
        );
        
        if !is_admin_action {
            if let Some(db) = &self.db {
                if let Ok(true) = db.is_muted(&username) {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Ack {
                            kind: AckKind::Failed,
                            info: "🔇 Você está silenciado e não pode enviar mensagens".to_string(),
                            message_id: None,
                        });
                    }
                    return;
                }
            }
        }
        
        match action {
            ChatAction::Text { from: _, content, timestamp } => {
                // Garantimos que o `from` seja o username autenticado
                if !self.state.check_rate_limit(&username, rate_limit_per_sec()) {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()), message_id: None });
                    }
                    return;
                }
                self.broadcast(username, content, timestamp);
            }
            ChatAction::RoomText { from: _, room, content, timestamp } => {
                if !self.state.check_rate_limit(&username, rate_limit_per_sec()) {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx.clone(), ChatMessage::Ack { kind: AckKind::Failed, info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()), message_id: None });
                    }
                    return;
                }
                let in_room = self.state.subscriptions.get(&username).map_or(false, |s| s.contains(&room));
                if in_room {
                    if let Some(members) = self.state.rooms.get(&room) {
                        let msg = ChatMessage::RoomText { from: username.clone(), room: room.clone(), content: content.clone(), timestamp };
                        for member in members.iter() {
                            let member_name = member.key();
                            if let Some(tx) = self.state.get_client_tx(member_name) {
                                if let Err(e) = tx.try_send(msg.clone()) {
                                    match e {
                                        TrySendError::Full(_) => metrics::inc_send_full(),
                                        _ => {}
                                    }
                                    error!(to=%member_name, error=%e, "Falha ao enviar RoomText");
                                }
                            }
                        }
                    }
                } else {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: format!("Você precisa entrar na sala {} para enviar mensagens.", room), message_id: None });
                    }
                }
            }
            ChatAction::Private { from: _, to, content, timestamp, message_id } => {
                if !self.state.check_rate_limit(&username, rate_limit_per_sec()) {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()), message_id: message_id.clone() });
                    }
                    return;
                }
                self.send_private_message(username, to, content, timestamp, message_id);
            }
            ChatAction::ListRequest { from: _ } => {
                if !self.state.check_rate_limit(&username, rate_limit_per_sec()) {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()), message_id: None });
                    }
                    return;
                }
                let users = self.state.list_usernames();
                if let Some(sender_tx) = self.state.get_client_tx(&username) {
                    self.send_reliable(sender_tx, ChatMessage::ListResponse { users });
                }
            }
            ChatAction::HistoryRequest { from: _, to, limit } => {
                if !self.state.check_rate_limit(&username, rate_limit_per_sec()) {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()), message_id: None });
                    }
                    return;
                }
                
                // Buscar histórico no banco de dados
                if let Some(db) = &self.db {
                    let db_clone = db.clone();
                    let username_clone = username.clone();
                    let to_clone = to.clone();
                    let limit_val = limit.unwrap_or(50); // Default: 50 mensagens
                    
                    // Busca em thread separada para não bloquear
                    let state_clone = self.state.clone();
                    tokio::task::spawn_blocking(move || {
                        let messages_result = if let Some(ref other_user) = to_clone {
                            // Buscar mensagens entre username e other_user
                            db_clone.get_private_messages(&username_clone, other_user, limit_val)
                        } else {
                            // Buscar todas as mensagens do usuário
                            db_clone.get_user_messages(&username_clone, limit_val)
                        };
                        
                        match messages_result {
                            Ok(rows) => {
                                // Converter MessageRecord para StoredMessage
                                let messages: Vec<StoredMessage> = rows.into_iter().map(|row| StoredMessage {
                                    id: row.id,
                                    from_user: row.from_user,
                                    to_user: row.to_user,
                                    room: row.room,
                                    content: row.content,
                                    timestamp: row.timestamp,
                                    message_id: row.message_id,
                                    message_type: row.message_type,
                                }).collect();
                                
                                // Enviar resposta
                                if let Some(tx) = state_clone.get_client_tx(&username_clone) {
                                    let response = ChatMessage::HistoryResponse { messages };
                                    let _ = tx.try_send(response);
                                }
                            }
                            Err(e) => {
                                debug!(error=%e, user=%username_clone, "Falha ao buscar histórico");
                                if let Some(tx) = state_clone.get_client_tx(&username_clone) {
                                    let err_msg = ChatMessage::Error(format!("Falha ao buscar histórico: {}", e));
                                    let _ = tx.try_send(err_msg);
                                }
                            }
                        }
                    });
                } else {
                    // Sem banco de dados configurado
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Error("Histórico não disponível (sem banco de dados)".to_string()));
                    }
                }
            }
            ChatAction::JoinRoom { room } => {
                if !self.state.check_rate_limit(&username, rate_limit_per_sec()) {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()), message_id: None });
                    }
                    return;
                }
                // Métricas: join room
                metrics::inc_room_joins();
                self.state.join_room(&username, &room);
                info!(user=%username, room=%room, action="join");
                if let Some(tx) = self.state.get_client_tx(&username) {
                    self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::System, info: format!("Entrou na sala {}", room), message_id: None });
                }
            }
            ChatAction::LeaveRoom { room } => {
                if !self.state.check_rate_limit(&username, rate_limit_per_sec()) {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: format!("Limite de mensagens excedido ({} msg/s).", rate_limit_per_sec()), message_id: None });
                    }
                    return;
                }
                // Métricas: leave room
                metrics::inc_room_leaves();
                self.state.leave_room(&username, &room);
                info!(user=%username, room=%room, action="leave");
                if let Some(tx) = self.state.get_client_tx(&username) {
                    self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::System, info: format!("Saiu da sala {}", room), message_id: None });
                }
            }
            ChatAction::Logout { .. } => {
                // Métricas: logout
                metrics::inc_logouts();
                metrics::set_users_online(self.state.client_count().saturating_sub(1) as i64);
                
                // Efetua logout das sessões do usuário (se AuthManager disponível)
                if let Some(auth) = &self.auth {
                    auth.logout_user(&username);
                }
                if let Some(tx) = self.state.get_client_tx(&username) {
                    self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::System, info: "Logout realizado".to_string(), message_id: None });
                }
            }
            ChatAction::ReadReceipt { message_id, reader } => {
                // Read receipt: notificar o remetente original que a mensagem foi lida
                // Precisamos encontrar quem enviou a mensagem original
                // Por simplicidade, vamos enviar para todos os usuários online
                // (em produção, você buscaria no DB quem enviou message_id)
                
                info!(reader=%reader, message_id=%message_id, "📖 Read receipt recebido");
                
                // Por enquanto, enviamos ACK para todos os usuários online
                // (assumindo que o remetente está online)
                for entry in self.state.clients.iter() {
                    let target_username = entry.key();
                    if target_username != &reader {
                        let tx = entry.value().1.clone();
                        let ack = ChatMessage::Ack {
                            kind: AckKind::Read,
                            info: format!("{} leu sua mensagem", reader),
                            message_id: Some(message_id.clone()),
                        };
                        let _ = tx.try_send(ack);
                    }
                }
            }
            ChatAction::Typing { user, typing } => {
                // Typing indicator: broadcast para todos os usuários online
                debug!(user=%user, typing=%typing, "⌨️ Typing indicator");
                
                let typing_msg = ChatMessage::Typing {
                    user: user.clone(),
                    typing,
                };
                
                for entry in self.state.clients.iter() {
                    let target_username = entry.key();
                    if target_username != &user {
                        let tx = entry.value().1.clone();
                        let _ = tx.try_send(typing_msg.clone());
                    }
                }
            }

            // ===== COMANDOS DE ADMINISTRAÇÃO =====
            
            ChatAction::AdminKick { target, reason } => {
                // Verificar se usuário é admin
                if let Some(db) = &self.db {
                    match db.is_admin(&username) {
                        Ok(true) => {
                            info!(admin=%username, target=%target, reason=%reason, "🔨 Admin KICK");
                            
                            // Log de moderação
                            let _ = db.log_moderation("kick", &username, &target, Some(&reason), None);
                            
                            // Desconectar usuário
                            self.state.remove_client_by_username(&target);
                            
                            // Notificar admin
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: format!("✅ {} foi expulso: {}", target, reason),
                                    message_id: None,
                                });
                            }
                            
                            // Notificar alvo (se ainda conectado)
                            if let Some(tx) = self.state.get_client_tx(&target) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::Failed,
                                    info: format!("Você foi expulso por {}: {}", username, reason),
                                    message_id: None,
                                });
                            }
                        }
                        Ok(false) => {
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::Failed,
                                    info: "❌ Sem permissão (requer admin)".to_string(),
                                    message_id: None,
                                });
                            }
                        }
                        Err(e) => {
                            error!(error=%e, "Erro ao verificar admin");
                        }
                    }
                } else {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Error("Banco de dados não disponível".to_string()));
                    }
                }
            }

            ChatAction::AdminBan { target, duration_secs, reason } => {
                if let Some(db) = &self.db {
                    match db.is_admin(&username) {
                        Ok(true) => {
                            // Verificar se alvo existe
                            if let Some(auth) = &self.auth {
                                if !auth.user_exists(&target) {
                                    if let Some(tx) = self.state.get_client_tx(&username) {
                                        self.send_reliable(tx, ChatMessage::Error(format!("Usuário '{}' não existe", target)));
                                    }
                                    return;
                                }
                            }
                            
                            info!(admin=%username, target=%target, duration=%duration_secs, reason=%reason, "🚫 Admin BAN");
                            
                            // Banir no banco
                            let duration_opt = if duration_secs > 0 { Some(duration_secs) } else { None };
                            let _ = db.ban_user(&target, &username, &reason, duration_opt);
                            
                            // Log de moderação
                            let details = format!("duração: {}s", duration_secs);
                            let _ = db.log_moderation("ban", &username, &target, Some(&reason), Some(&details));
                            
                            // Desconectar usuário
                            self.state.remove_client_by_username(&target);
                            
                            // Notificar admin
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                let duration_text = if duration_secs > 0 {
                                    format!("{} segundos", duration_secs)
                                } else {
                                    "permanente".to_string()
                                };
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: format!("✅ {} foi banido por {}: {}", target, duration_text, reason),
                                    message_id: None,
                                });
                            }
                        }
                        Ok(false) => {
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::Failed,
                                    info: "❌ Sem permissão (requer admin)".to_string(),
                                    message_id: None,
                                });
                            }
                        }
                        Err(e) => error!(error=%e, "Erro ao verificar admin"),
                    }
                } else {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Error("Banco de dados não disponível".to_string()));
                    }
                }
            }

            ChatAction::AdminMute { target, duration_secs } => {
                if let Some(db) = &self.db {
                    match db.is_admin(&username) {
                        Ok(true) => {
                            info!(admin=%username, target=%target, duration=%duration_secs, "🔇 Admin MUTE");
                            
                            // Silenciar no banco
                            let _ = db.mute_user(&target, &username, duration_secs);
                            
                            // Log
                            let details = format!("duração: {}s", duration_secs);
                            let _ = db.log_moderation("mute", &username, &target, None, Some(&details));
                            
                            // Notificar admin
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: format!("✅ {} foi silenciado por {} segundos", target, duration_secs),
                                    message_id: None,
                                });
                            }
                            
                            // Notificar alvo
                            if let Some(tx) = self.state.get_client_tx(&target) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: format!("Você foi silenciado por {} segundos", duration_secs),
                                    message_id: None,
                                });
                            }
                        }
                        Ok(false) => {
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::Failed,
                                    info: "❌ Sem permissão (requer admin)".to_string(),
                                    message_id: None,
                                });
                            }
                        }
                        Err(e) => error!(error=%e, "Erro ao verificar admin"),
                    }
                } else {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Error("Banco de dados não disponível".to_string()));
                    }
                }
            }

            ChatAction::AdminUnmute { target } => {
                if let Some(db) = &self.db {
                    match db.is_admin(&username) {
                        Ok(true) => {
                            info!(admin=%username, target=%target, "🔊 Admin UNMUTE");
                            
                            let _ = db.unmute_user(&target);
                            let _ = db.log_moderation("unmute", &username, &target, None, None);
                            
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: format!("✅ {} foi desilenciado", target),
                                    message_id: None,
                                });
                            }
                            
                            if let Some(tx) = self.state.get_client_tx(&target) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: "Você foi desilenciado".to_string(),
                                    message_id: None,
                                });
                            }
                        }
                        Ok(false) => {
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::Failed,
                                    info: "❌ Sem permissão (requer admin)".to_string(),
                                    message_id: None,
                                });
                            }
                        }
                        Err(e) => error!(error=%e, "Erro ao verificar admin"),
                    }
                } else {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Error("Banco de dados não disponível".to_string()));
                    }
                }
            }

            ChatAction::AdminPromote { target } => {
                if let Some(db) = &self.db {
                    match db.is_admin(&username) {
                        Ok(true) => {
                            // Verificar se alvo existe
                            if let Some(auth) = &self.auth {
                                if !auth.user_exists(&target) {
                                    if let Some(tx) = self.state.get_client_tx(&username) {
                                        self.send_reliable(tx, ChatMessage::Error(format!("Usuário '{}' não existe", target)));
                                    }
                                    return;
                                }
                            }
                            
                            info!(admin=%username, target=%target, "⬆️ Admin PROMOTE");
                            
                            let _ = db.promote_admin(&target, &username);
                            let _ = db.log_moderation("promote", &username, &target, None, None);
                            
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: format!("✅ {} foi promovido a admin", target),
                                    message_id: None,
                                });
                            }
                            
                            if let Some(tx) = self.state.get_client_tx(&target) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: "🎉 Você foi promovido a admin!".to_string(),
                                    message_id: None,
                                });
                            }
                        }
                        Ok(false) => {
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::Failed,
                                    info: "❌ Sem permissão (requer admin)".to_string(),
                                    message_id: None,
                                });
                            }
                        }
                        Err(e) => error!(error=%e, "Erro ao verificar admin"),
                    }
                } else {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Error("Banco de dados não disponível".to_string()));
                    }
                }
            }

            ChatAction::AdminDemote { target } => {
                if let Some(db) = &self.db {
                    match db.is_admin(&username) {
                        Ok(true) => {
                            info!(admin=%username, target=%target, "⬇️ Admin DEMOTE");
                            
                            let _ = db.demote_admin(&target);
                            let _ = db.log_moderation("demote", &username, &target, None, None);
                            
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: format!("✅ {} perdeu status de admin", target),
                                    message_id: None,
                                });
                            }
                            
                            if let Some(tx) = self.state.get_client_tx(&target) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: "Você perdeu status de admin".to_string(),
                                    message_id: None,
                                });
                            }
                        }
                        Ok(false) => {
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::Failed,
                                    info: "❌ Sem permissão (requer admin)".to_string(),
                                    message_id: None,
                                });
                            }
                        }
                        Err(e) => error!(error=%e, "Erro ao verificar admin"),
                    }
                } else {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Error("Banco de dados não disponível".to_string()));
                    }
                }
            }

            ChatAction::AdminList => {
                if let Some(db) = &self.db {
                    match db.list_admins() {
                        Ok(admins) => {
                            let list = if admins.is_empty() {
                                "Nenhum admin cadastrado".to_string()
                            } else {
                                format!("👮 Admins ({}): {}", admins.len(), admins.join(", "))
                            };
                            
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: list,
                                    message_id: None,
                                });
                            }
                        }
                        Err(e) => {
                            error!(error=%e, "Erro ao listar admins");
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Error("Erro ao buscar lista de admins".to_string()));
                            }
                        }
                    }
                } else {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Error("Banco de dados não disponível".to_string()));
                    }
                }
            }

            ChatAction::AdminLogs { limit } => {
                if let Some(db) = &self.db {
                    match db.is_admin(&username) {
                        Ok(true) => {
                            let limit_val = limit.unwrap_or(20);
                            match db.get_moderation_logs(limit_val) {
                                Ok(logs) => {
                                    if logs.is_empty() {
                                        if let Some(tx) = self.state.get_client_tx(&username) {
                                            self.send_reliable(tx, ChatMessage::Ack {
                                                kind: AckKind::System,
                                                info: "Nenhum log de moderação".to_string(),
                                                message_id: None,
                                            });
                                        }
                                    } else {
                                        let mut info = format!("📋 Logs de Moderação (últimos {}):\n", logs.len());
                                        for log in logs {
                                            info.push_str(&format!(
                                                "[{}] {} → {} ({}): {}\n",
                                                log.timestamp,
                                                log.moderator,
                                                log.target_user,
                                                log.action,
                                                log.reason.unwrap_or_else(|| log.details.unwrap_or_default())
                                            ));
                                        }
                                        
                                        if let Some(tx) = self.state.get_client_tx(&username) {
                                            self.send_reliable(tx, ChatMessage::Ack {
                                                kind: AckKind::System,
                                                info,
                                                message_id: None,
                                            });
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(error=%e, "Erro ao buscar logs");
                                    if let Some(tx) = self.state.get_client_tx(&username) {
                                        self.send_reliable(tx, ChatMessage::Error("Erro ao buscar logs".to_string()));
                                    }
                                }
                            }
                        }
                        Ok(false) => {
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Ack {
                                    kind: AckKind::Failed,
                                    info: "❌ Sem permissão (requer admin)".to_string(),
                                    message_id: None,
                                });
                            }
                        }
                        Err(e) => error!(error=%e, "Erro ao verificar admin"),
                    }
                } else {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Error("Banco de dados não disponível".to_string()));
                    }
                }
            }

            ChatAction::SearchMessages { query, limit, user_filter } => {
                if let Some(db) = &self.db {
                    let limit_val = limit.unwrap_or(50);
                    
                    match db.search_messages(&query, limit_val, user_filter.as_deref()) {
                        Ok(results) => {
                            info!(user=%username, query=%query, results=%results.len(), "🔍 Busca FTS5");
                            
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::SearchResponse {
                                    messages: results.clone(),
                                    total: results.len(),
                                });
                            }
                        }
                        Err(e) => {
                            error!(error=%e, user=%username, query=%query, "Erro na busca FTS5");
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Error(format!("Erro na busca: {}", e)));
                            }
                        }
                    }
                } else {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Error("Banco de dados não disponível".to_string()));
                    }
                }
            }

            ChatAction::SendFile { from, to, file_id, file_name, file_size, mime_type, timestamp } => {
                if let Some(db) = &self.db {
                    // Gerar token de download para o destinatário
                    match db.generate_download_token(&file_id, &to) {
                        Ok(token) => {
                            info!(from=%from, to=%to, file=%file_name, size=%file_size, "📎 Arquivo enviado");
                            
                            // Notificar destinatário
                            if let Some(to_tx) = self.state.get_client_tx(&to) {
                                self.send_reliable(to_tx, ChatMessage::FileNotification {
                                    from: from.clone(),
                                    file_id: file_id.clone(),
                                    file_name: file_name.clone(),
                                    file_size,
                                    mime_type: mime_type.clone(),
                                    download_token: token,
                                    timestamp: timestamp.to_rfc3339(),
                                });
                            }
                            
                            // Confirmar para remetente
                            if let Some(from_tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(from_tx, ChatMessage::Ack {
                                    kind: AckKind::System,
                                    info: format!("✅ Arquivo '{}' enviado para {}", file_name, to),
                                    message_id: None,
                                });
                            }
                        }
                        Err(e) => {
                            error!(error=%e, file_id=%file_id, "Erro ao gerar token de download");
                            if let Some(tx) = self.state.get_client_tx(&username) {
                                self.send_reliable(tx, ChatMessage::Error("Erro ao enviar arquivo".to_string()));
                            }
                        }
                    }
                } else {
                    if let Some(tx) = self.state.get_client_tx(&username) {
                        self.send_reliable(tx, ChatMessage::Error("Banco de dados não disponível".to_string()));
                    }
                }
            }
        }
    }

    pub fn register_user(&self, username: String, addr: SocketAddr, tx: Tx) {
        // Validação de username
        if let Err(err_msg) = ChatState::validate_username(&username) {
            warn!(username=%username, "Tentativa de registro com username inválido: {}", err_msg);
            self.send_reliable(tx.clone(), ChatMessage::Error(format!("Username inválido: {}", err_msg)));
            return;
        }

        // Verificar unicidade
        if self.state.is_username_taken(&username) {
            warn!(username=%username, "Tentativa de registro com username já em uso");
            self.send_reliable(tx.clone(), ChatMessage::Error(format!("Username '{}' já está em uso", username)));
            return;
        }

        // Usamos a função add_client do ChatState que já lida com a inserção
        self.state.add_client(addr, username.clone(), tx.clone());

        // Log de entrada: mostramos username. Se quiser ver endereços, defina `SHOW_ADDRS=1`.
        info!(username=%username, "📝 [LOG] entrou no chat");

        // Notificamos todos (Broadcast de boas-vindas)
        self.broadcast("Sistema".to_string(), format!("{} entrou na sala!", username), Utc::now());
    }

    /// Trata tentativa de login: autentica via AuthManager (se presente)
    /// Em caso de sucesso, registra cliente no `ChatState` e envia `SessionToken`.
    pub fn handle_login(&self, username: String, password: String, addr: SocketAddr, tx: Tx) {
        // Se não há AuthManager, fazemos o fluxo legado (registro direto)
        if self.auth.is_none() {
            self.register_user(username, addr, tx);
            return;
        }

        let auth = self.auth.as_ref().unwrap();
        match auth.login(&username, &password) {
            Ok(token) => {
                // Métricas: login bem-sucedido
                metrics::inc_logins();
                metrics::set_users_online(self.state.client_count() as i64);
                
                // Registra o cliente no estado (mapeia addr -> username e armazena tx)
                self.state.add_client(addr, username.clone(), tx.clone());

                // Envia SessionToken para o cliente
                self.send_reliable(tx.clone(), ChatMessage::SessionToken { token: token.clone(), username: username.clone() });

                // Envia Ack de sucesso
                self.send_reliable(tx.clone(), ChatMessage::Ack { kind: AckKind::System, info: "Login bem-sucedido".to_string(), message_id: None });

                // 🚀 FLUSH: Buscar e enviar mensagens pendentes
                if let Some(db) = &self.db {
                    let db_clone = db.clone();
                    let username_clone = username.clone();
                    let state_clone = self.state.clone();
                    
                    tokio::task::spawn_blocking(move || {
                        match db_clone.get_pending_messages(&username_clone) {
                            Ok(pending) => {
                                if !pending.is_empty() {
                                    info!(user=%username_clone, count=%pending.len(), "📬 Entregando mensagens offline");
                                    
                                    // 📬 Notificação antes das mensagens (evita "spam visual")
                                    if let Some(user_tx) = state_clone.get_client_tx(&username_clone) {
                                        let notify = ChatMessage::Ack {
                                            kind: AckKind::System,
                                            info: format!("📬 Você tem {} mensagens novas recebidas enquanto estava fora:", pending.len()),
                                            message_id: None,
                                        };
                                        let _ = user_tx.try_send(notify);
                                    }
                                    
                                    let mut delivered_ids = Vec::new();
                                    
                                    // Enviar cada mensagem pendente
                                    for msg in pending {
                                        if let Some(user_tx) = state_clone.get_client_tx(&username_clone) {
                                            // Converter timestamp string para DateTime
                                            let timestamp = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&msg.timestamp) {
                                                dt.with_timezone(&chrono::Utc)
                                            } else {
                                                chrono::Utc::now()
                                            };
                                            
                                            let chat_msg = ChatMessage::Private {
                                                from: msg.from_user.clone(),
                                                to: username_clone.clone(),
                                                content: msg.content.clone(),
                                                timestamp,
                                                message_id: msg.message_id.clone(),
                                            };
                                            
                                            // Tenta enviar via canal
                                            if user_tx.try_send(chat_msg).is_ok() {
                                                delivered_ids.push(msg.id);
                                            } else {
                                                warn!(msg_id=%msg.id, "Falha ao enviar mensagem pendente (canal cheio)");
                                            }
                                        }
                                    }
                                    
                                    // Marcar mensagens como entregues
                                    if !delivered_ids.is_empty() {
                                        if let Err(e) = db_clone.mark_messages_delivered(&delivered_ids) {
                                            error!(error=%e, "Falha ao marcar mensagens como entregues");
                                        } else {
                                            // Métricas: mensagens offline entregues
                                            metrics::inc_offline_messages_delivered();
                                            info!(count=%delivered_ids.len(), "✅ Mensagens offline entregues e marcadas");
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!(error=%e, user=%username_clone, "Falha ao buscar mensagens pendentes");
                            }
                        }
                    });
                }
            }
            Err(e) => {
                // Mapear mensagens de erro para respostas mais claras de UX
                let info = if e.contains("não encontrado") {
                    format!("Usuário '{}' não encontrado", username)
                } else if e.to_lowercase().contains("password incorreto") || e.to_lowercase().contains("senha incorreta") {
                    format!("Senha incorreta para o usuário '{}'", username)
                } else {
                    e.clone()
                };

                // Envia Ack de falha para o tx recebido
                self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info, message_id: None });
            }
        }
    }

    /// Trata tentativa de registro (Register). Responde com Ack System/Failed usando o tx fornecido.
    pub fn handle_register(&self, username: String, password: String, _addr: SocketAddr, tx: Tx) {
        if self.auth.is_none() {
            // Sem AuthManager, registro não suportado — respondemos com Failed
            self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: "Registro não suportado no servidor".to_string(), message_id: None });
            return;
        }

        let auth = self.auth.as_ref().unwrap();
        match auth.register(&username, &password) {
            Ok(()) => {
                // Métricas: registro bem-sucedido
                metrics::inc_registrations();
                
                // Tentamos auto-login após registro para melhorar UX
                match auth.login(&username, &password) {
                    Ok(token) => {
                        // Métricas: login automático após registro
                        metrics::inc_logins();
                        metrics::set_users_online(self.state.client_count() as i64);
                        
                        // Registra o cliente no estado
                        self.state.add_client(_addr, username.clone(), tx.clone());

                        // Envia SessionToken e Ack de sucesso combinados
                        self.send_reliable(tx.clone(), ChatMessage::SessionToken { token: token.clone(), username: username.clone() });
                        self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::System, info: "Registro realizado e login efetuado".to_string(), message_id: None });
                    }
                    Err(e) => {
                        // Registro foi ok, mas falha no auto-login (improvável)
                        self.send_reliable(tx.clone(), ChatMessage::Ack { kind: AckKind::System, info: "Registro realizado com sucesso".to_string(), message_id: None });
                        self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: format!("Falha no login automático: {}", e), message_id: None });
                    }
                }
            }
            Err(e) => {
                self.send_reliable(tx, ChatMessage::Ack { kind: AckKind::Failed, info: e, message_id: None });
            }
        }
    }

    pub fn disconnect_user(&self, addr: SocketAddr) {
        if let Some(username) = self.state.remove_client(&addr){
            info!(username=%username, "📝 [LOG] saiu do chat");
            self.broadcast("Sistema".to_string(), format!("{} saiu da sala!", username), Utc::now());
        };
    }
    pub fn broadcast(&self, from: String, content: String, timestamp: DateTime<Utc>) {
        // Métricas: mensagem broadcast/room
        metrics::inc_messages_sent();
        metrics::inc_room_messages();
        
        // Salva broadcast no banco de dados (não bloqueia)
        if let Some(db) = &self.db {
            let db_clone = db.clone();
            let from_clone = from.clone();
            let content_clone = content.clone();
            let timestamp_clone = timestamp.clone();
            
            tokio::task::spawn_blocking(move || {
                if let Err(e) = db_clone.insert_message(
                    &from_clone,
                    None,
                    None,
                    &content_clone,
                    &timestamp_clone,
                    None,
                    MessageType::Broadcast,
                ) {
                    debug!(error=%e, "Falha ao salvar broadcast no banco");
                }
            });
        }

        let msg = ChatMessage::Text { from, content, timestamp };
        for entry in self.state.clients.iter() {
            let username = entry.key();
            let tx = entry.value().1.clone();
            if let Err(e) = tx.try_send(msg.clone()) {
                match e {
                    TrySendError::Full(_) => metrics::inc_send_full(),
                    _ => {}
                }
                error!(to=%username, error=%e, "Falha ao enviar broadcast");
            }
        }
    }

    pub fn send_to(&self, to: String, msg: ChatMessage) {
        // Use o helper `get_client_tx` do `ChatState` para procurar pelo username
        if let Some(tx) = self.state.get_client_tx(&to) {
            if let Err(e) = tx.try_send(msg) {
                error!(to=%to, error=%e, "Falha ao enviar mensagem privada");
            }
        } else {
            warn!(to=%to, "Destinatário não encontrado");
        }
    }
}
