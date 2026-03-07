use messaging_domain::*;
use tracing::{info, warn};

/// Use Case: Enviar mensagem direta
pub struct SendDirectMessage<M, O, P>
where
    M: MessageRepository,
    O: OfflineQueueRepository,
    P: UserPresenceService,
{
    message_repo: M,
    offline_queue: O,
    presence: P,
}

impl<M, O, P> SendDirectMessage<M, O, P>
where
    M: MessageRepository,
    O: OfflineQueueRepository,
    P: UserPresenceService,
{
    pub fn new(message_repo: M, offline_queue: O, presence: P) -> Self {
        Self {
            message_repo,
            offline_queue,
            presence,
        }
    }

    pub fn execute(&self, message: DirectMessage) -> MessagingResult<DeliveryStatus> {
        // 1. Salvar mensagem no banco
        self.message_repo.save(&message)?;

        // 2. Verificar se destinatário está online
        if self.presence.is_online(&message.to) {
            info!(
                from = %message.from,
                to = %message.to,
                "Mensagem direta enviada (usuário online)"
            );
            Ok(DeliveryStatus::Delivered)
        } else {
            // 3. Se offline, adicionar à fila
            self.offline_queue.enqueue(&message)?;
            warn!(
                from = %message.from,
                to = %message.to,
                "Usuário offline - mensagem enfileirada"
            );
            Ok(DeliveryStatus::Queued)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryStatus {
    Delivered,
    Queued,
}

/// Use Case: Entregar mensagens pendentes
pub struct DeliverPendingMessages<O>
where
    O: OfflineQueueRepository,
{
    offline_queue: O,
}

impl<O> DeliverPendingMessages<O>
where
    O: OfflineQueueRepository,
{
    pub fn new(offline_queue: O) -> Self {
        Self { offline_queue }
    }

    pub fn execute(&self, username: &str) -> MessagingResult<Vec<PendingMessage>> {
        let pending = self.offline_queue.get_pending(username)?;

        if !pending.is_empty() {
            info!(
                user = %username,
                count = pending.len(),
                "Entregando mensagens offline"
            );
        }

        Ok(pending)
    }

    pub fn acknowledge(&self, message_id: &str) -> MessagingResult<()> {
        self.offline_queue.dequeue(message_id)
    }
}

/// Use Case: Marcar mensagem como lida
pub struct MarkMessageAsRead<M>
where
    M: MessageRepository,
{
    message_repo: M,
}

impl<M> MarkMessageAsRead<M>
where
    M: MessageRepository,
{
    pub fn new(message_repo: M) -> Self {
        Self { message_repo }
    }

    pub fn execute(&self, message_id: &str) -> MessagingResult<()> {
        self.message_repo.mark_read(message_id)?;
        info!(message_id = %message_id, "Mensagem marcada como lida");
        Ok(())
    }
}
