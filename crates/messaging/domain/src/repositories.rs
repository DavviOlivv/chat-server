use crate::entities::{DirectMessage, PendingMessage};
use crate::errors::MessagingResult;

/// Repository para persistência de mensagens diretas
pub trait MessageRepository: Send + Sync {
    /// Salva uma mensagem direta no banco
    fn save(&self, message: &DirectMessage) -> MessagingResult<()>;

    /// Busca mensagens entre dois usuários
    fn find_between_users(
        &self,
        user1: &str,
        user2: &str,
        limit: usize,
    ) -> MessagingResult<Vec<DirectMessage>>;

    /// Marca mensagem como entregue
    fn mark_delivered(&self, message_id: &str) -> MessagingResult<()>;

    /// Marca mensagem como lida
    fn mark_read(&self, message_id: &str) -> MessagingResult<()>;
}

/// Repository para fila de mensagens offline
pub trait OfflineQueueRepository: Send + Sync {
    /// Adiciona mensagem à fila offline
    fn enqueue(&self, message: &DirectMessage) -> MessagingResult<()>;

    /// Busca mensagens pendentes para um usuário
    fn get_pending(&self, username: &str) -> MessagingResult<Vec<PendingMessage>>;

    /// Remove mensagem da fila após entrega
    fn dequeue(&self, message_id: &str) -> MessagingResult<()>;

    /// Limpa mensagens antigas da fila
    fn cleanup_old(&self, days: u32) -> MessagingResult<usize>;
}

/// Service para verificar se usuário está online
pub trait UserPresenceService: Send + Sync {
    fn is_online(&self, username: &str) -> bool;
}
