use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum MessagingError {
    #[error("Usuário não encontrado: {0}")]
    UserNotFound(String),

    #[error("Usuário offline: {0}")]
    UserOffline(String),

    #[error("Mensagem duplicada: {0}")]
    DuplicateMessage(String),

    #[error("Limite de taxa excedido para usuário: {0}")]
    RateLimitExceeded(String),

    #[error("Erro de persistência: {0}")]
    PersistenceError(String),

    #[error("Erro de entrega: {0}")]
    DeliveryError(String),

    #[error("Erro interno: {0}")]
    InternalError(String),
}

pub type MessagingResult<T> = Result<T, MessagingError>;
