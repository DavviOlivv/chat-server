use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Representa uma mensagem direta entre dois usuários
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectMessage {
    pub message_id: String,
    pub from: String,
    pub to: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub delivered: bool,
    pub read: bool,
}

impl DirectMessage {
    pub fn new(
        message_id: String,
        from: String,
        to: String,
        content: String,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            message_id,
            from,
            to,
            content,
            timestamp,
            delivered: false,
            read: false,
        }
    }

    pub fn mark_delivered(&mut self) {
        self.delivered = true;
    }

    pub fn mark_read(&mut self) {
        self.read = true;
    }
}

/// Representa uma mensagem offline aguardando entrega
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMessage {
    pub id: i64,
    pub message_id: String,
    pub from_user: String,
    pub to_user: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub attempts: u32,
}

impl PendingMessage {
    pub fn from_direct_message(dm: &DirectMessage, id: i64) -> Self {
        Self {
            id,
            message_id: dm.message_id.clone(),
            from_user: dm.from.clone(),
            to_user: dm.to.clone(),
            content: dm.content.clone(),
            timestamp: dm.timestamp,
            attempts: 0,
        }
    }

    pub fn increment_attempts(&mut self) {
        self.attempts += 1;
    }
}
