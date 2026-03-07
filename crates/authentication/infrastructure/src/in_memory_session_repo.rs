use auth_domain::*;
use dashmap::DashMap;
use std::sync::Arc;

/// Implementação em memória do SessionRepository usando DashMap
pub struct InMemorySessionRepository {
    sessions: Arc<DashMap<String, Session>>,
}

impl InMemorySessionRepository {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
        }
    }

    pub fn with_dashmap(sessions: Arc<DashMap<String, Session>>) -> Self {
        Self { sessions }
    }
}

impl Default for InMemorySessionRepository {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionRepository for InMemorySessionRepository {
    fn save(&self, session: &Session) -> AuthResult<()> {
        self.sessions.insert(session.token.clone(), session.clone());
        Ok(())
    }

    fn find_by_token(&self, token: &str) -> AuthResult<Option<Session>> {
        Ok(self.sessions.get(token).map(|entry| entry.value().clone()))
    }

    fn delete(&self, token: &str) -> AuthResult<bool> {
        Ok(self.sessions.remove(token).is_some())
    }

    fn delete_by_username(&self, username: &str) -> AuthResult<usize> {
        let to_remove: Vec<String> = self
            .sessions
            .iter()
            .filter(|entry| entry.value().username == username)
            .map(|entry| entry.key().clone())
            .collect();

        let count = to_remove.len();
        for token in to_remove {
            self.sessions.remove(&token);
        }

        Ok(count)
    }

    fn count(&self) -> AuthResult<usize> {
        Ok(self.sessions.len())
    }

    fn cleanup_expired(&self, ttl_secs: u64) -> AuthResult<usize> {
        let to_remove: Vec<String> = self
            .sessions
            .iter()
            .filter(|entry| entry.value().is_expired(ttl_secs))
            .map(|entry| entry.key().clone())
            .collect();

        let count = to_remove.len();
        for token in to_remove {
            self.sessions.remove(&token);
        }

        Ok(count)
    }
}
