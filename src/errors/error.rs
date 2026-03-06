use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChatError {
    #[error("Falha na rede: {0}")]
    NetworkError(#[from] io::Error),

    #[error("Usuário '{0}' já está em uso")]
    UsernameTaken(String),

    #[error("Mensagem inválida ou corrompida")]
    InvalidMessage,

    #[error("Desconectado do servidor")]
    Disconnected,
}
