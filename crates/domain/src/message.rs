use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum AckKind {
    Received,
    Delivered,
    Read,
    Failed,
    System,
}

/// Mensagem armazenada no banco de dados
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StoredMessage {
    pub id: i64,
    pub from_user: String,
    pub to_user: Option<String>,
    pub room: Option<String>,
    pub content: String,
    pub timestamp: String,
    pub message_id: Option<String>,
    pub message_type: String,
}

/// Ações que o cliente pode realizar (payload do envelope Authenticated)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ChatAction {
    Text {
        from: String,
        content: String,
        timestamp: DateTime<Utc>,
    },

    /// Mensagem enviada para uma sala (room)
    RoomText {
        from: String,
        room: String,
        content: String,
        timestamp: DateTime<Utc>,
    },

    Private {
        from: String,
        to: String,
        content: String,
        timestamp: DateTime<Utc>,
        message_id: Option<String>,
    },

    /// Pedido para listar usuários online
    ListRequest {
        from: String,
    },

    /// Pedido de histórico de mensagens
    HistoryRequest {
        from: String,
        to: Option<String>,
        limit: Option<usize>,
    },

    /// Pedido para entrar em uma sala
    JoinRoom {
        room: String,
    },

    /// Pedido para sair de uma sala
    LeaveRoom {
        room: String,
    },

    Logout {
        username: String,
    },

    /// Read receipt - confirma que mensagem foi lida
    ReadReceipt {
        message_id: String,
        reader: String,
    },

    /// Typing indicator - notifica que usuário está digitando
    Typing {
        user: String,
        typing: bool,
    },

    // ===== Comandos de Administração =====
    /// Expulsar usuário da sala/servidor
    AdminKick {
        target: String,
        reason: String,
    },

    /// Banir usuário temporariamente
    AdminBan {
        target: String,
        duration_secs: u64,
        reason: String,
    },

    /// Silenciar usuário (não pode enviar mensagens)
    AdminMute {
        target: String,
        duration_secs: u64,
    },

    /// Remover silenciamento
    AdminUnmute {
        target: String,
    },

    /// Promover usuário a admin
    AdminPromote {
        target: String,
    },

    /// Remover status de admin
    AdminDemote {
        target: String,
    },

    /// Listar todos os admins
    AdminList,

    /// Visualizar logs de moderação
    AdminLogs {
        limit: Option<usize>,
    },

    // ===== Busca de Mensagens =====
    /// Buscar mensagens usando Full-Text Search
    SearchMessages {
        query: String,
        limit: Option<usize>,
        user_filter: Option<String>, // Filtrar por usuário específico
    },

    // ===== Transferência de Arquivos =====
    /// Notificar destinatário sobre arquivo enviado
    SendFile {
        from: String,
        to: String,
        file_id: String,
        file_name: String,
        file_size: u64,
        mime_type: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ChatMessage {
    /// Registro de novo usuário (não requer autenticação)
    Register {
        username: String,
        password: String,
    },

    /// Login com credenciais (não requer autenticação)
    Login {
        username: String,
        password: String,
    },

    /// Resposta do servidor com token de sessão após login bem-sucedido
    SessionToken {
        token: String,
        username: String,
    },

    /// Envelope para ações autenticadas - requer token válido
    Authenticated {
        token: String,
        action: Box<ChatAction>,
    },

    // Mensagens legadas sem autenticação (para compatibilidade temporária)
    // TODO: Remover após migração completa
    Text {
        from: String,
        content: String,
        timestamp: DateTime<Utc>,
    },

    RoomText {
        from: String,
        room: String,
        content: String,
        timestamp: DateTime<Utc>,
    },

    Private {
        from: String,
        to: String,
        content: String,
        timestamp: DateTime<Utc>,
        message_id: Option<String>,
    },

    ListRequest {
        from: String,
    },

    JoinRoom {
        room: String,
    },

    LeaveRoom {
        room: String,
    },

    Logout {
        username: String,
    },

    /// Read receipt - confirma que mensagem foi lida
    ReadReceipt {
        message_id: String,
        reader: String,
    },

    /// Typing indicator - notifica que usuário está digitando
    Typing {
        user: String,
        typing: bool,
    },

    // Respostas do servidor (não requerem autenticação para receber)
    Ack {
        kind: AckKind,
        info: String,
        message_id: Option<String>,
    },

    ListResponse {
        users: Vec<String>,
    },

    HistoryResponse {
        messages: Vec<StoredMessage>,
    },

    /// Resposta de busca Full-Text Search
    SearchResponse {
        messages: Vec<SearchResult>,
        total: usize,
    },

    /// Notificação de arquivo recebido
    FileNotification {
        from: String,
        file_id: String,
        file_name: String,
        file_size: u64,
        mime_type: String,
        download_token: String,
        timestamp: String,
    },

    Error(String),
}

/// Resultado de busca FTS5 com ranking
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchResult {
    pub id: i64,
    pub from_user: String,
    pub to_user: Option<String>,
    pub room: Option<String>,
    pub content: String,
    pub timestamp: String,
    pub rank: f64,       // Relevância do resultado
    pub snippet: String, // Trecho destacado
}
