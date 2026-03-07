use auth_domain::*;
use tracing::{info, warn};
use uuid::Uuid;

/// Use Case: Registrar novo usuário
pub struct RegisterUser<U: UserRepository> {
    user_repo: U,
}

impl<U: UserRepository> RegisterUser<U> {
    pub fn new(user_repo: U) -> Self {
        Self { user_repo }
    }

    pub fn execute(&self, credentials: Credentials) -> AuthResult<User> {
        // Validar username
        if credentials.username.len() < 3 || credentials.username.len() > 20 {
            return Err(AuthError::InvalidUsername(
                "Username deve ter entre 3 e 20 caracteres".to_string(),
            ));
        }

        // Validar senha
        if credentials.password.len() < 6 {
            return Err(AuthError::WeakPassword);
        }

        // Verificar se já existe
        if self.user_repo.exists(&credentials.username)? {
            return Err(AuthError::UserAlreadyExists(credentials.username.clone()));
        }

        // Hash da senha
        let password_hash = bcrypt::hash(&credentials.password, bcrypt::DEFAULT_COST)
            .map_err(|e| AuthError::InternalError(format!("Falha ao hash senha: {}", e)))?;

        let user = User::new(credentials.username.clone(), password_hash);

        // Persistir
        self.user_repo.save(&user)?;

        info!(username = %credentials.username, "Usuário registrado com sucesso");
        Ok(user)
    }
}

/// Use Case: Fazer login e obter token
pub struct LoginUser<U: UserRepository, S: SessionRepository> {
    user_repo: U,
    session_repo: S,
}

impl<U: UserRepository, S: SessionRepository> LoginUser<U, S> {
    pub fn new(user_repo: U, session_repo: S) -> Self {
        Self {
            user_repo,
            session_repo,
        }
    }

    pub fn execute(&self, credentials: Credentials) -> AuthResult<String> {
        // Buscar usuário
        let user = self
            .user_repo
            .find_by_username(&credentials.username)?
            .ok_or_else(|| AuthError::UserNotFound(credentials.username.clone()))?;

        // Verificar senha
        let valid = bcrypt::verify(&credentials.password, &user.password_hash)
            .map_err(|e| AuthError::InternalError(format!("Erro ao verificar senha: {}", e)))?;

        if !valid {
            warn!(username = %credentials.username, "Tentativa de login com senha incorreta");
            return Err(AuthError::InvalidPassword);
        }

        // Gerar token
        let token = Uuid::new_v4().to_string();
        let session = Session::new(token.clone(), user.username.clone());

        // Salvar sessão
        self.session_repo.save(&session)?;

        info!(username = %user.username, "Login bem-sucedido");
        Ok(token)
    }
}

/// Use Case: Validar token e obter username
pub struct ValidateToken<S: SessionRepository> {
    session_repo: S,
    ttl_secs: u64,
}

impl<S: SessionRepository> ValidateToken<S> {
    pub fn new(session_repo: S, ttl_secs: u64) -> Self {
        Self {
            session_repo,
            ttl_secs,
        }
    }

    pub fn execute(&self, token: &str) -> AuthResult<String> {
        let session = self
            .session_repo
            .find_by_token(token)?
            .ok_or(AuthError::InvalidToken)?;

        if session.is_expired(self.ttl_secs) {
            self.session_repo.delete(token)?;
            return Err(AuthError::InvalidToken);
        }

        Ok(session.username)
    }
}

/// Use Case: Fazer logout
pub struct LogoutUser<S: SessionRepository> {
    session_repo: S,
}

impl<S: SessionRepository> LogoutUser<S> {
    pub fn new(session_repo: S) -> Self {
        Self { session_repo }
    }

    pub fn execute_by_token(&self, token: &str) -> AuthResult<bool> {
        self.session_repo.delete(token)
    }

    pub fn execute_by_username(&self, username: &str) -> AuthResult<usize> {
        self.session_repo.delete_by_username(username)
    }
}
