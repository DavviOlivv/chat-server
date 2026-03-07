// Adapter: Database -> MessageRepository & OfflineQueueRepository
use crate::server::database::{Database, MessageType};
use chrono::Utc;
use messaging_domain::*;
use std::sync::Arc;

pub struct DatabaseMessageAdapter {
    db: Arc<Database>,
}

impl DatabaseMessageAdapter {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl MessageRepository for DatabaseMessageAdapter {
    fn save(&self, message: &DirectMessage) -> MessagingResult<()> {
        self.db
            .insert_message_with_delivery(
                &message.from,
                Some(&message.to),
                None,
                &message.content,
                &message.timestamp,
                Some(&message.message_id),
                MessageType::Private,
                message.delivered,
            )
            .map(|_| ()) // Descarta o ID retornado
            .map_err(|e| MessagingError::PersistenceError(e.to_string()))
    }

    fn find_between_users(
        &self,
        user1: &str,
        user2: &str,
        limit: usize,
    ) -> MessagingResult<Vec<DirectMessage>> {
        // Database não tem método específico, retorna vazio por enquanto
        // TODO: implementar busca de mensagens entre usuários
        let _ = (user1, user2, limit);
        Ok(Vec::new())
    }

    fn mark_delivered(&self, message_id: &str) -> MessagingResult<()> {
        // Database usa ID numérico, não message_id string
        // Por enquanto, retorna OK (implementação futura pode buscar por message_id)
        let _ = message_id;
        Ok(())
    }

    fn mark_read(&self, message_id: &str) -> MessagingResult<()> {
        // Database não tem método mark_read ainda
        let _ = message_id;
        Ok(())
    }
}

impl OfflineQueueRepository for DatabaseMessageAdapter {
    fn enqueue(&self, message: &DirectMessage) -> MessagingResult<()> {
        // Marca como não entregue ao salvar
        let mut msg = message.clone();
        msg.delivered = false;
        self.save(&msg)
    }

    fn get_pending(&self, username: &str) -> MessagingResult<Vec<PendingMessage>> {
        let records = self
            .db
            .get_pending_messages(username)
            .map_err(|e| MessagingError::PersistenceError(e.to_string()))?;

        Ok(records
            .into_iter()
            .map(|r| PendingMessage {
                id: r.id,
                message_id: r.message_id.unwrap_or_else(|| r.id.to_string()),
                from_user: r.from_user,
                to_user: r.to_user.unwrap_or_default(),
                content: r.content,
                timestamp: r.timestamp.parse().unwrap_or_else(|_| Utc::now()),
                attempts: 0,
            })
            .collect())
    }

    fn dequeue(&self, message_id: &str) -> MessagingResult<()> {
        // Por enquanto, não deleta mensagens pendentes
        // (Database não tem método delete_pending_message por message_id)
        let _ = message_id;
        Ok(())
    }

    fn cleanup_old(&self, days: u32) -> MessagingResult<usize> {
        self.db
            .delete_pending_messages_older_than(days as i64)
            .map_err(|e| MessagingError::PersistenceError(e.to_string()))
    }
}
