use crate::entities::{Session, User};
use crate::errors::AuthResult;

/// Repository abstrato para persistência de usuários
pub trait UserRepository: Send + Sync {
    fn save(&self, user: &User) -> AuthResult<()>;
    fn find_by_username(&self, username: &str) -> AuthResult<Option<User>>;
    fn exists(&self, username: &str) -> AuthResult<bool>;
    fn count(&self) -> AuthResult<usize>;
}

/// Repository abstrato para gerenciamento de sessões
pub trait SessionRepository: Send + Sync {
    fn save(&self, session: &Session) -> AuthResult<()>;
    fn find_by_token(&self, token: &str) -> AuthResult<Option<Session>>;
    fn delete(&self, token: &str) -> AuthResult<bool>;
    fn delete_by_username(&self, username: &str) -> AuthResult<usize>;
    fn count(&self) -> AuthResult<usize>;
    fn cleanup_expired(&self, ttl_secs: u64) -> AuthResult<usize>;
}
