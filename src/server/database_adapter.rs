// Adapter: Database -> UserRepository
use crate::server::database::Database;
use auth_domain::*;
use std::sync::Arc;

pub struct DatabaseUserAdapter {
    db: Arc<Database>,
}

impl DatabaseUserAdapter {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }
}

impl UserRepository for DatabaseUserAdapter {
    fn save(&self, user: &User) -> AuthResult<()> {
        self.db
            .insert_user(&user.username, &user.password_hash)
            .map_err(|e| {
                let error_msg = e.to_string();
                if error_msg.contains("UNIQUE") || error_msg.contains("já existe") {
                    AuthError::UserAlreadyExists(user.username.clone())
                } else {
                    AuthError::PersistenceError(error_msg)
                }
            })
    }

    fn find_by_username(&self, username: &str) -> AuthResult<Option<User>> {
        match self.db.get_user(username) {
            Ok(Some(user_record)) => Ok(Some(User::new(
                user_record.username,
                user_record.password_hash,
            ))),
            Ok(None) => Ok(None),
            Err(e) => Err(AuthError::PersistenceError(e.to_string())),
        }
    }

    fn exists(&self, username: &str) -> AuthResult<bool> {
        match self.db.get_user(username) {
            Ok(opt) => Ok(opt.is_some()),
            Err(e) => Err(AuthError::PersistenceError(e.to_string())),
        }
    }

    fn count(&self) -> AuthResult<usize> {
        // Database não tem método count, então podemos retornar estimativa ou implementar depois
        Ok(0) // TODO: implementar count no Database
    }
}
