use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum AuthError {
    #[error("Usuário já existe: {0}")]
    UserAlreadyExists(String),

    #[error("Usuário não encontrado: {0}")]
    UserNotFound(String),

    #[error("Senha inválida")]
    InvalidPassword,

    #[error("Token inválido ou expirado")]
    InvalidToken,

    #[error("Nome de usuário inválido: {0}")]
    InvalidUsername(String),

    #[error("Senha muito fraca")]
    WeakPassword,

    #[error("Erro de persistência: {0}")]
    PersistenceError(String),

    #[error("Erro interno: {0}")]
    InternalError(String),
}

pub type AuthResult<T> = Result<T, AuthError>;
