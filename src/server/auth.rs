use crate::server::database::Database;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use prometheus::{register_counter, register_gauge, Counter, Gauge};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Estrutura para persistência de usuários (legacy JSON)
#[derive(Debug, Serialize, Deserialize, Clone)]
struct UserRecord {
    username: String,
    password_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UsersFile {
    users: Vec<UserRecord>,
}

#[derive(Debug, Clone)]
struct SessionRecord {
    username: String,
    created_at: DateTime<Utc>,
}

// Prometheus metrics
static ACTIVE_SESSIONS: Lazy<Gauge> =
    Lazy::new(|| register_gauge!("chat_active_sessions", "Número de sessões ativas").unwrap());

static EXPIRED_SESSIONS_TOTAL: Lazy<Counter> = Lazy::new(|| {
    register_counter!("chat_expired_sessions_total", "Total de sessões expiradas").unwrap()
});

pub struct AuthManager {
    /// Database SQLite (novo sistema de persistência)
    db: Arc<Database>,
    /// Mapa de token -> SessionRecord (username + created_at) - mantido em memória
    sessions: Arc<DashMap<String, SessionRecord>>,
    /// Caminho do arquivo JSON legado (para migração)
    users_file_path: String,
}

impl AuthManager {
    pub fn new(users_file_path: &str) -> Self {
        // Determina caminho do banco (usa DB_PATH ou deriva de users_file_path)
        let db_path =
            std::env::var("DB_PATH").unwrap_or_else(|_| users_file_path.replace(".json", ".db"));

        let db = Arc::new(Database::new(&db_path).expect("Falha ao abrir banco de dados"));

        let manager = Self {
            db,
            sessions: Arc::new(DashMap::new()),
            users_file_path: users_file_path.to_string(),
        };

        // Migra dados do JSON para SQLite se existir
        manager.migrate_from_json();

        // Apenas spawn cleanup em produção (fora de testes)
        if std::env::var("SKIP_TOKEN_CLEANUP").is_err()
            && tokio::runtime::Handle::try_current().is_ok()
        {
            let sessions_clone = manager.sessions.clone();
            let ttl_secs: u64 = std::env::var("TOKEN_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60 * 60 * 24);
            let cleanup_interval_secs: u64 = std::env::var("TOKEN_CLEANUP_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60);
            let ttl = Duration::from_secs(ttl_secs);
            let cleanup_interval = Duration::from_secs(cleanup_interval_secs);

            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(cleanup_interval).await;
                    let now = Utc::now();

                    // Coleta tokens expirados ANTES de remover (evita deadlock no DashMap)
                    let to_remove: Vec<String> = sessions_clone
                        .iter()
                        .filter(|entry| {
                            now.signed_duration_since(entry.value().created_at)
                                .num_seconds() as u64
                                >= ttl.as_secs()
                        })
                        .map(|entry| entry.key().clone())
                        .collect();

                    let removed = to_remove.len();
                    for token in to_remove {
                        sessions_clone.remove(&token);
                    }

                    if removed > 0 {
                        // Atualiza métricas
                        EXPIRED_SESSIONS_TOTAL.inc_by(removed as f64);
                        ACTIVE_SESSIONS.set(sessions_clone.len() as f64);
                        info!(removed=%removed, "🧹 Limpeza periódica: tokens expirados removidos");
                    }
                }
            });
        }
        manager
    }

    /// Migra usuários do JSON para SQLite (apenas uma vez)
    fn migrate_from_json(&self) {
        let path = Path::new(&self.users_file_path);
        if !path.exists() {
            info!("🔐 Sem arquivo JSON para migrar");
            return;
        }

        // Verifica se já existem usuários no banco
        if let Ok(count) = self.db.count_users() {
            if count > 0 {
                info!("🔐 Banco já contém {} usuários, pulando migração", count);
                return;
            }
        }

        // Lê e migra do JSON
        match fs::read_to_string(path) {
            Ok(content) => {
                match serde_json::from_str::<UsersFile>(&content) {
                    Ok(users_file) => {
                        let mut migrated = 0;
                        for record in users_file.users {
                            if self.db.insert_user(&record.username, &record.password_hash).is_ok()
                            {
                                migrated += 1;
                            }
                        }
                        info!(
                            "🔄 Migrados {} usuários de {} para SQLite",
                            migrated, self.users_file_path
                        );

                        // Opcional: renomeia arquivo JSON para backup
                        let backup_path = format!("{}.backup", self.users_file_path);
                        if let Err(e) = fs::rename(&self.users_file_path, &backup_path) {
                            warn!("Falha ao criar backup do JSON: {}", e);
                        } else {
                            info!("📦 Backup do JSON criado em {}", backup_path);
                        }
                    }
                    Err(e) => {
                        error!("❌ Falha ao parsear arquivo JSON para migração: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("❌ Falha ao ler arquivo JSON para migração: {}", e);
            }
        }
    }

    /// Registra novo usuário com hash bcrypt
    pub fn register(&self, username: &str, password: &str) -> Result<(), String> {
        // Validação básica
        if username.is_empty() || password.is_empty() {
            return Err("Username e password não podem ser vazios".to_string());
        }

        if username.len() < 3 {
            return Err("Username deve ter no mínimo 3 caracteres".to_string());
        }

        if password.len() < 6 {
            return Err("Password deve ter no mínimo 6 caracteres".to_string());
        }

        // Verifica se usuário já existe no banco
        if let Ok(Some(_)) = self.db.get_user(username) {
            return Err(format!("Usuário '{}' já existe", username));
        }

        // Gera hash bcrypt (custo configurável via BCRYPT_COST, default 12)
        let cost: u32 = std::env::var("BCRYPT_COST")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(12u32);
        let hash =
            bcrypt::hash(password, cost).map_err(|e| format!("Falha ao gerar hash: {}", e))?;

        // Persiste no banco
        self.db
            .insert_user(username, &hash)
            .map_err(|e| format!("Falha ao salvar usuário no banco: {}", e))?;

        info!("✅ Usuário '{}' registrado com sucesso", username);
        Ok(())
    }

    /// Autentica usuário e retorna token de sessão
    pub fn login(&self, username: &str, password: &str) -> Result<String, String> {
        // Busca usuário no banco
        let user = self
            .db
            .get_user(username)
            .map_err(|e| format!("Erro ao buscar usuário: {}", e))?
            .ok_or_else(|| format!("Usuário '{}' não encontrado", username))?;

        // Verifica password com bcrypt
        let valid = bcrypt::verify(password, &user.password_hash)
            .map_err(|e| format!("Falha ao verificar password: {}", e))?;

        if !valid {
            warn!("🔒 Tentativa de login falhou para usuário '{}'", username);
            return Err("Password incorreto".to_string());
        }

        // Verificar se usuário está banido
        if let Ok(banned) = self.db.is_banned(username) {
            if banned {
                info!("🚫 Tentativa de login de usuário banido: {}", username);
                return Err("Você está banido deste servidor".to_string());
            }
        }

        // Gera token de sessão (UUID v4)
        let token = Uuid::new_v4().to_string();

        // Salva sessão com timestamp
        let rec = SessionRecord {
            username: username.to_string(),
            created_at: Utc::now(),
        };
        self.sessions.insert(token.clone(), rec);
        // Atualiza métrica de sessões ativas
        ACTIVE_SESSIONS.set(self.sessions.len() as f64);

        info!("🔓 Usuário '{}' autenticado - token gerado", username);
        Ok(token)
    }

    /// Valida token e retorna username associado
    pub fn validate_token(&self, token: &str) -> Option<String> {
        // Valida token e verifica expiração conforme TOKEN_TTL_SECS
        let mut is_expired = false;
        let username = if let Some(entry) = self.sessions.get(token) {
            let rec = entry.value();
            let ttl_secs: u64 = std::env::var("TOKEN_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60 * 60 * 24);

            if Utc::now()
                .signed_duration_since(rec.created_at)
                .num_seconds() as u64
                >= ttl_secs
            {
                is_expired = true;
                None
            } else {
                Some(rec.username.clone())
            }
        } else {
            None
        }; // O lock de leitura do 'entry' morre aqui!

        if is_expired {
            self.sessions.remove(token); // Agora o lock de escrita está livre
                                         // Métricas: token expirado
            EXPIRED_SESSIONS_TOTAL.inc_by(1.0);
            ACTIVE_SESSIONS.set(self.sessions.len() as f64);
        }

        username
    }

    /// Remove sessão (logout)
    pub fn logout(&self, token: &str) -> bool {
        match self.sessions.remove(token) {
            Some((_, rec)) => {
                info!("👋 Logout de usuário '{}'", rec.username);
                // Atualiza métricas
                ACTIVE_SESSIONS.set(self.sessions.len() as f64);
                true
            }
            None => false,
        }
    }

    /// Remove todas as sessões de um usuário específico
    pub fn logout_user(&self, username: &str) {
        let tokens_to_remove: Vec<String> = self
            .sessions
            .iter()
            .filter(|entry| entry.value().username == username)
            .map(|entry| entry.key().clone())
            .collect();

        let count = tokens_to_remove.len();

        for token in &tokens_to_remove {
            self.sessions.remove(token);
        }

        if count > 0 {
            info!("👋 Removidas {} sessões do usuário '{}'", count, username);
            // Atualiza métricas
            ACTIVE_SESSIONS.set(self.sessions.len() as f64);
        }
    }

    /// Retorna número de usuários registrados
    pub fn user_count(&self) -> usize {
        self.db.count_users().unwrap_or(0)
    }

    /// Verifica se um usuário existe no banco de dados
    pub fn user_exists(&self, username: &str) -> bool {
        self.db.get_user(username).unwrap_or(None).is_some()
    }

    /// Retorna número de sessões ativas
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn create_test_auth() -> AuthManager {
        // Usa banco em memória para testes
        env::set_var("DB_PATH", ":memory:");
        env::set_var("SKIP_TOKEN_CLEANUP", "1");
        AuthManager::new("test_users_temp.json")
    }

    #[test]
    fn test_register_and_login() {
        let auth = create_test_auth();

        // Registro
        assert!(auth.register("alice", "senha123").is_ok());
        assert_eq!(auth.user_count(), 1);

        // Tentar registrar usuário duplicado
        assert!(auth.register("alice", "outra_senha").is_err());

        // Login com senha correta
        let token = auth
            .login("alice", "senha123")
            .expect("Login deveria funcionar");
        assert!(!token.is_empty());
        assert_eq!(auth.session_count(), 1);

        // Validar token
        assert_eq!(auth.validate_token(&token), Some("alice".to_string()));

        // Login com senha incorreta
        assert!(auth.login("alice", "senha_errada").is_err());
    }

    #[test]
    fn test_logout() {
        let auth = create_test_auth();

        auth.register("bob", "password123").unwrap();
        let token = auth.login("bob", "password123").unwrap();

        assert_eq!(auth.session_count(), 1);

        // Logout
        assert!(auth.logout(&token));
        assert_eq!(auth.session_count(), 0);

        // Token não é mais válido
        assert_eq!(auth.validate_token(&token), None);
    }

    #[test]
    fn test_validation() {
        let auth = create_test_auth();

        // Username muito curto
        assert!(auth.register("ab", "password123").is_err());

        // Password muito curta
        assert!(auth.register("charlie", "12345").is_err());

        // Vazio
        assert!(auth.register("", "password").is_err());
        assert!(auth.register("dave", "").is_err());
    }

    #[tokio::test]
    async fn test_token_expiration_short_ttl() {
        use std::time::Duration;

        // Desabilita cleanup automático em testes
        std::env::set_var("SKIP_TOKEN_CLEANUP", "1");
        std::env::set_var("DB_PATH", ":memory:");
        // Configure TTL curto para o teste
        std::env::set_var("TOKEN_TTL_SECS", "1");
        // Use bcrypt cost baixo para teste para acelerar hashing
        std::env::set_var("BCRYPT_COST", "4");

        let auth = AuthManager::new("test_ttl.json");

        // Registro e Login
        auth.register("ttl_user", "senha123").unwrap();
        let token = auth.login("ttl_user", "senha123").unwrap();

        // Verificação imediata
        assert!(
            auth.validate_token(&token).is_some(),
            "token deve ser válido imediatamente após login"
        );

        // Aguarda a expiração (1.5s para garantir que passou do TTL de 1s)
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // A validação manual deve falhar agora (validate_token verifica TTL e remove se expirado)
        let result = auth.validate_token(&token);
        assert!(
            result.is_none(),
            "O token deveria ter expirado, mas retornou: {:?}",
            result
        );

        // Cleanup env vars
        std::env::remove_var("SKIP_TOKEN_CLEANUP");
        std::env::remove_var("TOKEN_TTL_SECS");
        std::env::remove_var("BCRYPT_COST");
        std::env::remove_var("DB_PATH");
    }
}
