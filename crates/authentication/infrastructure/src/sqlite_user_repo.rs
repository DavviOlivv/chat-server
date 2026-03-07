use auth_domain::*;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::{Arc, Mutex};
use tracing::warn;

/// Implementação SQLite do UserRepository
pub struct SqliteUserRepository {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteUserRepository {
    pub fn new(db_path: &str) -> AuthResult<Self> {
        let conn = Connection::open(db_path).map_err(|e| {
            AuthError::PersistenceError(format!("Falha ao abrir banco: {}", e))
        })?;

        // Criar tabela se não existir
        conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                username TEXT PRIMARY KEY,
                password_hash TEXT NOT NULL
            )",
            [],
        )
        .map_err(|e| {
            AuthError::PersistenceError(format!("Falha ao criar tabela users: {}", e))
        })?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub fn with_connection(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }
}

impl UserRepository for SqliteUserRepository {
    fn save(&self, user: &User) -> AuthResult<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO users (username, password_hash) VALUES (?1, ?2)",
            params![&user.username, &user.password_hash],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                AuthError::UserAlreadyExists(user.username.clone())
            } else {
                AuthError::PersistenceError(format!("Falha ao salvar usuário: {}", e))
            }
        })?;

        Ok(())
    }

    fn find_by_username(&self, username: &str) -> AuthResult<Option<User>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT username, password_hash FROM users WHERE username = ?1")
            .map_err(|e| {
                AuthError::PersistenceError(format!("Falha ao preparar query: {}", e))
            })?;

        let result = stmt
            .query_row(params![username], |row| {
                Ok(User {
                    username: row.get(0)?,
                    password_hash: row.get(1)?,
                })
            })
            .optional()
            .map_err(|e| {
                warn!("Erro ao buscar usuário {}: {}", username, e);
                AuthError::PersistenceError(format!("Falha ao buscar usuário: {}", e))
            })?;

        Ok(result)
    }

    fn exists(&self, username: &str) -> AuthResult<bool> {
        Ok(self.find_by_username(username)?.is_some())
    }

    fn count(&self) -> AuthResult<usize> {
        let conn = self.conn.lock().unwrap();

        let count: usize = conn
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .map_err(|e| {
                AuthError::PersistenceError(format!("Falha ao contar usuários: {}", e))
            })?;

        Ok(count)
    }
}
