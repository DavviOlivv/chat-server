pub mod model;
pub mod server;
pub mod errors;
pub mod client; 
pub mod utils;

// Re-export common types for easier access from binaries
pub use model::message::ChatMessage;
pub use model::message::ChatAction;
pub use model::message::AckKind;