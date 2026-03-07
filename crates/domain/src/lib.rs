pub mod message;

// Re-exportar tipos principais para facilitar imports
pub use message::{
    AckKind, ChatAction, ChatMessage, SearchResult, StoredMessage,
};
