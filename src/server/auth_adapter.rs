// Adapter: converte entre SessionRecord (legacy) e Session (domain)
use crate::server::auth::SessionRecord;
use auth_domain::Session;
use dashmap::DashMap;
use std::sync::Arc;

impl SessionRecord {
    pub fn to_session(&self, token: String) -> Session {
        Session {
            token,
            username: self.username.clone(),
            created_at: self.created_at,
        }
    }

    pub fn from_session(session: &Session) -> Self {
        Self {
            username: session.username.clone(),
            created_at: session.created_at,
        }
    }
}

/// Adapter para DashMap<String, SessionRecord> que implementa SessionRepository
pub struct DashMapSessionAdapter {
    sessions: Arc<DashMap<String, SessionRecord>>,
}

impl DashMapSessionAdapter {
    pub fn new(sessions: Arc<DashMap<String, SessionRecord>>) -> Self {
        Self { sessions }
    }
}

impl auth_domain::SessionRepository for DashMapSessionAdapter {
    fn save(&self, session: &Session) -> auth_domain::AuthResult<()> {
        self.sessions
            .insert(session.token.clone(), SessionRecord::from_session(session));
        Ok(())
    }

    fn find_by_token(&self, token: &str) -> auth_domain::AuthResult<Option<Session>> {
        Ok(self
            .sessions
            .get(token)
            .map(|entry| entry.value().to_session(token.to_string())))
    }

    fn delete(&self, token: &str) -> auth_domain::AuthResult<bool> {
        Ok(self.sessions.remove(token).is_some())
    }

    fn delete_by_username(&self, username: &str) -> auth_domain::AuthResult<usize> {
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

    fn count(&self) -> auth_domain::AuthResult<usize> {
        Ok(self.sessions.len())
    }

    fn cleanup_expired(&self, ttl_secs: u64) -> auth_domain::AuthResult<usize> {
        use chrono::Utc;
        let now = Utc::now();

        let to_remove: Vec<String> = self
            .sessions
            .iter()
            .filter(|entry| {
                now.signed_duration_since(entry.value().created_at)
                    .num_seconds() as u64
                    >= ttl_secs
            })
            .map(|entry| entry.key().clone())
            .collect();

        let count = to_remove.len();
        for token in to_remove {
            self.sessions.remove(&token);
        }

        Ok(count)
    }
}
