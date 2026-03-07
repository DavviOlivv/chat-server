use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Entidade User - representa um usuário no domínio
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct User {
    pub username: String,
    pub password_hash: String,
}

impl User {
    pub fn new(username: String, password_hash: String) -> Self {
        Self {
            username,
            password_hash,
        }
    }
}

/// Value Object - representa credenciais do usuário
#[derive(Debug, Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

impl Credentials {
    pub fn new(username: String, password: String) -> Self {
        Self { username, password }
    }
}

/// Entidade Session - representa uma sessão ativa
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub username: String,
    pub created_at: DateTime<Utc>,
}

impl Session {
    pub fn new(token: String, username: String) -> Self {
        Self {
            token,
            username,
            created_at: Utc::now(),
        }
    }

    pub fn is_expired(&self, ttl_secs: u64) -> bool {
        let now = Utc::now();
        now.signed_duration_since(self.created_at).num_seconds() as u64 >= ttl_secs
    }
}
