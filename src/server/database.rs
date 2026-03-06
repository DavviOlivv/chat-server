use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

/// Wrapper thread-safe para conexão SQLite
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Cria uma nova conexão com o banco de dados
    ///
    /// # Argumentos
    /// - `path`: caminho do arquivo de banco (use ":memory:" para testes)
    pub fn new(path: &str) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.run_migrations()?;
        info!(db_path=%path, "📦 Banco de dados SQLite inicializado");
        Ok(db)
    }

    /// Executa migrations do schema
    fn run_migrations(&self) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();

        // Tabela de usuários
        conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                username TEXT PRIMARY KEY NOT NULL,
                password_hash TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        )?;

        // Tabela de mensagens
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                from_user TEXT NOT NULL,
                to_user TEXT,
                room TEXT,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                message_id TEXT UNIQUE,
                message_type TEXT NOT NULL,
                delivered BOOLEAN DEFAULT 0
            )",
            [],
        )?;

        // Migration: Adicionar coluna delivered se não existir (para bancos antigos)
        let _ = conn.execute(
            "ALTER TABLE messages ADD COLUMN delivered BOOLEAN DEFAULT 0",
            [],
        );
        // Ignora erro se coluna já existir

        // Índices para performance
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_from ON messages(from_user)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_to ON messages(to_user)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_room ON messages(room)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp)",
            [],
        )?;

        // Índice crítico para buscar mensagens pendentes rapidamente
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_pending ON messages(to_user, delivered)",
            [],
        )?;

        // Tabela de administradores
        conn.execute(
            "CREATE TABLE IF NOT EXISTS admins (
                username TEXT PRIMARY KEY NOT NULL,
                promoted_at TEXT NOT NULL,
                promoted_by TEXT NOT NULL,
                FOREIGN KEY (username) REFERENCES users(username)
            )",
            [],
        )?;

        // Tabela de banimentos
        conn.execute(
            "CREATE TABLE IF NOT EXISTS bans (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL,
                banned_by TEXT NOT NULL,
                reason TEXT NOT NULL,
                banned_at TEXT NOT NULL,
                expires_at TEXT,
                active BOOLEAN DEFAULT 1,
                FOREIGN KEY (username) REFERENCES users(username),
                FOREIGN KEY (banned_by) REFERENCES users(username)
            )",
            [],
        )?;

        // Tabela de silenciamentos (mutes)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS mutes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL,
                muted_by TEXT NOT NULL,
                muted_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                active BOOLEAN DEFAULT 1,
                FOREIGN KEY (username) REFERENCES users(username),
                FOREIGN KEY (muted_by) REFERENCES users(username)
            )",
            [],
        )?;

        // Tabela de logs de moderação
        conn.execute(
            "CREATE TABLE IF NOT EXISTS moderation_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                action TEXT NOT NULL,
                moderator TEXT NOT NULL,
                target_user TEXT NOT NULL,
                reason TEXT,
                details TEXT,
                timestamp TEXT NOT NULL,
                FOREIGN KEY (moderator) REFERENCES users(username),
                FOREIGN KEY (target_user) REFERENCES users(username)
            )",
            [],
        )?;

        // Índices para tabelas de moderação
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_bans_username ON bans(username, active)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_mutes_username ON mutes(username, active)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_modlogs_moderator ON moderation_logs(moderator)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_modlogs_target ON moderation_logs(target_user)",
            [],
        )?;

        // ===== Full-Text Search (FTS5) =====

        // Tabela virtual FTS5 para busca de mensagens
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
                content,
                from_user,
                to_user,
                room,
                timestamp,
                content='messages',
                content_rowid='id'
            )",
            [],
        )?;

        // Trigger: Inserir na FTS quando mensagem é criada
        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
                INSERT INTO messages_fts(rowid, content, from_user, to_user, room, timestamp)
                VALUES (new.id, new.content, new.from_user, new.to_user, new.room, new.timestamp);
            END",
            [],
        )?;

        // Trigger: Atualizar FTS quando mensagem é atualizada
        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN
                UPDATE messages_fts 
                SET content = new.content,
                    from_user = new.from_user,
                    to_user = new.to_user,
                    room = new.room,
                    timestamp = new.timestamp
                WHERE rowid = old.id;
            END",
            [],
        )?;

        // Trigger: Deletar da FTS quando mensagem é deletada
        conn.execute(
            "CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
                DELETE FROM messages_fts WHERE rowid = old.id;
            END",
            [],
        )?;

        // ===== Transferência de Arquivos =====

        // Tabela de metadados de arquivos
        conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                file_id TEXT PRIMARY KEY,
                filename TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                mime_type TEXT NOT NULL,
                uploaded_by TEXT NOT NULL,
                uploaded_at TEXT NOT NULL,
                file_path TEXT NOT NULL,
                download_token TEXT,
                token_expires_at TEXT,
                FOREIGN KEY(uploaded_by) REFERENCES users(username)
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_uploaded_by ON files(uploaded_by)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_token ON files(download_token)",
            [],
        )?;

        debug!("✅ Migrations executadas");
        Ok(())
    }

    // ============ OPERAÇÕES DE USUÁRIOS ============

    /// Insere um novo usuário
    pub fn insert_user(&self, username: &str, password_hash: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO users (username, password_hash, created_at) VALUES (?1, ?2, ?3)",
            params![username, password_hash, now],
        )?;

        debug!(username=%username, "👤 Usuário inserido no banco");
        Ok(())
    }

    /// Busca um usuário por username
    pub fn get_user(&self, username: &str) -> SqlResult<Option<UserRecord>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare("SELECT username, password_hash, created_at FROM users WHERE username = ?1")?;

        let user = stmt
            .query_row(params![username], |row| {
                Ok(UserRecord {
                    username: row.get(0)?,
                    password_hash: row.get(1)?,
                    created_at: row.get(2)?,
                })
            })
            .optional()?;

        Ok(user)
    }

    /// Lista todos os usuários
    pub fn list_users(&self) -> SqlResult<Vec<String>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare("SELECT username FROM users ORDER BY created_at")?;
        let users = stmt
            .query_map([], |row| row.get(0))?
            .collect::<SqlResult<Vec<String>>>()?;

        Ok(users)
    }

    /// Conta quantos usuários existem
    pub fn count_users(&self) -> SqlResult<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Remove um usuário (usado em testes principalmente)
    pub fn delete_user(&self, username: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM users WHERE username = ?1", params![username])?;
        debug!(username=%username, "🗑️  Usuário removido do banco");
        Ok(())
    }

    // ============ OPERAÇÕES DE MENSAGENS ============

    /// Insere uma mensagem no banco
    pub fn insert_message(
        &self,
        from_user: &str,
        to_user: Option<&str>,
        room: Option<&str>,
        content: &str,
        timestamp: &DateTime<Utc>,
        message_id: Option<&str>,
        message_type: MessageType,
    ) -> SqlResult<i64> {
        self.insert_message_with_delivery(
            from_user,
            to_user,
            room,
            content,
            timestamp,
            message_id,
            message_type,
            false,
        )
    }

    /// Insere uma mensagem no banco com status de entrega
    pub fn insert_message_with_delivery(
        &self,
        from_user: &str,
        to_user: Option<&str>,
        room: Option<&str>,
        content: &str,
        timestamp: &DateTime<Utc>,
        message_id: Option<&str>,
        message_type: MessageType,
        delivered: bool,
    ) -> SqlResult<i64> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO messages (from_user, to_user, room, content, timestamp, message_id, message_type, delivered)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                from_user,
                to_user,
                room,
                content,
                timestamp.to_rfc3339(),
                message_id,
                message_type.as_str(),
                delivered,
            ],
        )?;

        let id = conn.last_insert_rowid();
        debug!(id=%id, from=%from_user, msg_type=%message_type.as_str(), delivered=%delivered, "💾 Mensagem salva");
        Ok(id)
    }

    /// Busca mensagens pendentes (não entregues) para um usuário
    pub fn get_pending_messages(&self, to_user: &str) -> SqlResult<Vec<MessageRecord>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, from_user, to_user, room, content, timestamp, message_id, message_type, delivered
             FROM messages
             WHERE to_user = ?1 AND delivered = 0
             ORDER BY timestamp ASC"
        )?;

        let messages = stmt
            .query_map(params![to_user], |row| {
                Ok(MessageRecord {
                    id: row.get(0)?,
                    from_user: row.get(1)?,
                    to_user: row.get(2)?,
                    room: row.get(3)?,
                    content: row.get(4)?,
                    timestamp: row.get(5)?,
                    message_id: row.get(6)?,
                    message_type: row.get(7)?,
                    delivered: row.get(8)?,
                })
            })?
            .collect::<SqlResult<Vec<_>>>()?;

        debug!(to_user=%to_user, count=%messages.len(), "📬 Mensagens pendentes buscadas");
        Ok(messages)
    }

    /// Marca uma mensagem como entregue
    pub fn mark_message_delivered(&self, message_id: i64) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "UPDATE messages SET delivered = 1 WHERE id = ?1",
            params![message_id],
        )?;

        debug!(id=%message_id, "✅ Mensagem marcada como entregue");
        Ok(())
    }

    /// Marca múltiplas mensagens como entregues (mais eficiente)
    pub fn mark_messages_delivered(&self, message_ids: &[i64]) -> SqlResult<()> {
        if message_ids.is_empty() {
            return Ok(());
        }

        let conn = self.conn.lock().unwrap();
        let placeholders = message_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "UPDATE messages SET delivered = 1 WHERE id IN ({})",
            placeholders
        );

        let mut stmt = conn.prepare(&query)?;
        let params: Vec<&dyn rusqlite::ToSql> = message_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        stmt.execute(params.as_slice())?;

        debug!(count=%message_ids.len(), "✅ Mensagens marcadas como entregues");
        Ok(())
    }

    /// Busca mensagens privadas entre dois usuários
    pub fn get_private_messages(
        &self,
        user1: &str,
        user2: &str,
        limit: usize,
    ) -> SqlResult<Vec<MessageRecord>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, from_user, to_user, room, content, timestamp, message_id, message_type, COALESCE(delivered, 0)
             FROM messages
             WHERE message_type = 'private'
               AND ((from_user = ?1 AND to_user = ?2) OR (from_user = ?2 AND to_user = ?1))
             ORDER BY timestamp DESC
             LIMIT ?3"
        )?;

        let messages = stmt
            .query_map(params![user1, user2, limit as i64], |row| {
                Ok(MessageRecord {
                    id: row.get(0)?,
                    from_user: row.get(1)?,
                    to_user: row.get(2)?,
                    room: row.get(3)?,
                    content: row.get(4)?,
                    timestamp: row.get(5)?,
                    message_id: row.get(6)?,
                    message_type: row.get(7)?,
                    delivered: row.get(8)?,
                })
            })?
            .collect::<SqlResult<Vec<_>>>()?;

        Ok(messages)
    }

    /// Busca mensagens de uma sala
    pub fn get_room_messages(&self, room: &str, limit: usize) -> SqlResult<Vec<MessageRecord>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, from_user, to_user, room, content, timestamp, message_id, message_type, COALESCE(delivered, 0)
             FROM messages
             WHERE room = ?1
             ORDER BY timestamp DESC
             LIMIT ?2"
        )?;

        let messages = stmt
            .query_map(params![room, limit as i64], |row| {
                Ok(MessageRecord {
                    id: row.get(0)?,
                    from_user: row.get(1)?,
                    to_user: row.get(2)?,
                    room: row.get(3)?,
                    content: row.get(4)?,
                    timestamp: row.get(5)?,
                    message_id: row.get(6)?,
                    message_type: row.get(7)?,
                    delivered: row.get(8)?,
                })
            })?
            .collect::<SqlResult<Vec<_>>>()?;

        Ok(messages)
    }

    /// Busca todas as mensagens de um usuário
    pub fn get_user_messages(&self, username: &str, limit: usize) -> SqlResult<Vec<MessageRecord>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, from_user, to_user, room, content, timestamp, message_id, message_type, COALESCE(delivered, 0)
             FROM messages
             WHERE from_user = ?1 OR to_user = ?1
             ORDER BY timestamp DESC
             LIMIT ?2"
        )?;

        let messages = stmt
            .query_map(params![username, limit as i64], |row| {
                Ok(MessageRecord {
                    id: row.get(0)?,
                    from_user: row.get(1)?,
                    to_user: row.get(2)?,
                    room: row.get(3)?,
                    content: row.get(4)?,
                    timestamp: row.get(5)?,
                    message_id: row.get(6)?,
                    message_type: row.get(7)?,
                    delivered: row.get(8)?,
                })
            })?
            .collect::<SqlResult<Vec<_>>>()?;

        Ok(messages)
    }

    /// Conta o total de mensagens
    pub fn count_messages(&self) -> SqlResult<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Conta mensagens pendentes (não entregues) para um usuário
    /// Usa o índice (to_user, delivered) para performance O(log n)
    pub fn count_pending_messages(&self, to_user: &str) -> SqlResult<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE to_user = ?1 AND delivered = 0",
            params![to_user],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Remove mensagens antigas (cleanup/GDPR)
    pub fn delete_messages_older_than(&self, days: i64) -> SqlResult<usize> {
        let conn = self.conn.lock().unwrap();
        let cutoff = Utc::now() - chrono::Duration::days(days);

        let deleted = conn.execute(
            "DELETE FROM messages WHERE timestamp < ?1",
            params![cutoff.to_rfc3339()],
        )?;

        info!(deleted=%deleted, days=%days, "🗑️  Mensagens antigas removidas");
        Ok(deleted)
    }

    /// Remove mensagens pendentes antigas (TTL para mensagens offline)
    pub fn delete_pending_messages_older_than(&self, days: i64) -> SqlResult<usize> {
        let conn = self.conn.lock().unwrap();
        let cutoff = Utc::now() - chrono::Duration::days(days);

        let deleted = conn.execute(
            "DELETE FROM messages WHERE delivered = 0 AND timestamp < ?1",
            params![cutoff.to_rfc3339()],
        )?;

        info!(deleted=%deleted, days=%days, "🗑️  Mensagens pendentes antigas removidas (TTL)");
        Ok(deleted)
    }
}

/// Registro de usuário do banco
#[derive(Debug, Clone)]
pub struct UserRecord {
    pub username: String,
    pub password_hash: String,
    pub created_at: String,
}

/// Registro de mensagem do banco
#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub id: i64,
    pub from_user: String,
    pub to_user: Option<String>,
    pub room: Option<String>,
    pub content: String,
    pub timestamp: String,
    pub message_id: Option<String>,
    pub message_type: String,
    pub delivered: bool,
}

/// Tipo de mensagem
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Private,
    Broadcast,
    Room,
}

impl MessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageType::Private => "private",
            MessageType::Broadcast => "broadcast",
            MessageType::Room => "room",
        }
    }
}

impl Database {
    // ============ OPERAÇÕES DE ADMINISTRAÇÃO ============

    /// Verifica se usuário é admin
    pub fn is_admin(&self, username: &str) -> SqlResult<bool> {
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM admins WHERE username = ?1",
                params![username],
                |_| Ok(true),
            )
            .unwrap_or(false);
        Ok(exists)
    }

    /// Promove usuário a admin
    pub fn promote_admin(&self, username: &str, promoted_by: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO admins (username, promoted_at, promoted_by) VALUES (?1, ?2, ?3)",
            params![username, now, promoted_by],
        )?;
        Ok(())
    }

    /// Remove status de admin
    pub fn demote_admin(&self, username: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM admins WHERE username = ?1", params![username])?;
        Ok(())
    }

    /// Lista todos os admins
    pub fn list_admins(&self) -> SqlResult<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT username FROM admins ORDER BY promoted_at")?;
        let admins = stmt
            .query_map([], |row| row.get(0))?
            .collect::<SqlResult<Vec<String>>>()?;
        Ok(admins)
    }

    /// Bane usuário
    pub fn ban_user(
        &self,
        username: &str,
        banned_by: &str,
        reason: &str,
        duration_secs: Option<u64>,
    ) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        let banned_at = now.to_rfc3339();
        let expires_at =
            duration_secs.map(|secs| (now + chrono::Duration::seconds(secs as i64)).to_rfc3339());

        conn.execute(
            "INSERT INTO bans (username, banned_by, reason, banned_at, expires_at, active)
             VALUES (?1, ?2, ?3, ?4, ?5, 1)",
            params![username, banned_by, reason, banned_at, expires_at],
        )?;
        Ok(())
    }

    /// Verifica se usuário está banido (e não expirou)
    pub fn is_banned(&self, username: &str) -> SqlResult<bool> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        let banned: bool = conn
            .query_row(
                "SELECT 1 FROM bans 
             WHERE username = ?1 
             AND active = 1 
             AND (expires_at IS NULL OR expires_at > ?2)",
                params![username, now],
                |_| Ok(true),
            )
            .unwrap_or(false);

        Ok(banned)
    }

    /// Remove banimento
    pub fn unban_user(&self, username: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE bans SET active = 0 WHERE username = ?1 AND active = 1",
            params![username],
        )?;
        Ok(())
    }

    /// Silencia usuário (mute)
    pub fn mute_user(&self, username: &str, muted_by: &str, duration_secs: u64) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        let muted_at = now.to_rfc3339();
        let expires_at = (now + chrono::Duration::seconds(duration_secs as i64)).to_rfc3339();

        conn.execute(
            "INSERT INTO mutes (username, muted_by, muted_at, expires_at, active)
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![username, muted_by, muted_at, expires_at],
        )?;
        Ok(())
    }

    /// Verifica se usuário está silenciado (e não expirou)
    pub fn is_muted(&self, username: &str) -> SqlResult<bool> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        let muted: bool = conn
            .query_row(
                "SELECT 1 FROM mutes 
             WHERE username = ?1 
             AND active = 1 
             AND expires_at > ?2",
                params![username, now],
                |_| Ok(true),
            )
            .unwrap_or(false);

        Ok(muted)
    }

    /// Remove silenciamento
    pub fn unmute_user(&self, username: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE mutes SET active = 0 WHERE username = ?1 AND active = 1",
            params![username],
        )?;
        Ok(())
    }

    /// Registra ação de moderação
    pub fn log_moderation(
        &self,
        action: &str,
        moderator: &str,
        target_user: &str,
        reason: Option<&str>,
        details: Option<&str>,
    ) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let timestamp = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO moderation_logs (action, moderator, target_user, reason, details, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![action, moderator, target_user, reason, details, timestamp],
        )?;
        Ok(())
    }

    /// Obtém logs de moderação recentes
    pub fn get_moderation_logs(&self, limit: usize) -> SqlResult<Vec<ModerationLog>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT action, moderator, target_user, reason, details, timestamp 
             FROM moderation_logs 
             ORDER BY id DESC 
             LIMIT ?1",
        )?;

        let logs = stmt
            .query_map(params![limit], |row| {
                Ok(ModerationLog {
                    action: row.get(0)?,
                    moderator: row.get(1)?,
                    target_user: row.get(2)?,
                    reason: row.get(3)?,
                    details: row.get(4)?,
                    timestamp: row.get(5)?,
                })
            })?
            .collect::<SqlResult<Vec<_>>>()?;

        Ok(logs)
    }

    /// Busca mensagens usando FTS5 (Full-Text Search)
    ///
    /// Suporta:
    /// - Boolean operators: "rust AND tokio", "chat OR server"
    /// - Phrase search: "\"async runtime\""
    /// - NOT operator: "rust NOT tokio"
    /// - Proximity: "NEAR(rust tokio, 5)"
    ///
    /// Retorna resultados ordenados por relevância (rank)
    pub fn search_messages(
        &self,
        query: &str,
        limit: usize,
        user_filter: Option<&str>,
    ) -> SqlResult<Vec<crate::model::message::SearchResult>> {
        let conn = self.conn.lock().unwrap();

        // Query base com FTS5 MATCH
        let mut sql = String::from(
            "SELECT 
                m.id,
                m.from_user,
                m.to_user,
                m.room,
                m.content,
                m.timestamp,
                rank,
                snippet(messages_fts, 0, '**', '**', '...', 32) as snippet
             FROM messages_fts
             INNER JOIN messages m ON messages_fts.rowid = m.id
             WHERE messages_fts MATCH ?",
        );

        // Adiciona filtro de usuário se especificado
        if user_filter.is_some() {
            sql.push_str(" AND (m.from_user = ? OR m.to_user = ?)");
        }

        // Ordena por relevância (rank DESC) e limita resultados
        sql.push_str(" ORDER BY rank LIMIT ?");

        let mut stmt = conn.prepare(&sql)?;

        // Função helper para mapear row
        let map_row = |row: &rusqlite::Row| -> SqlResult<crate::model::message::SearchResult> {
            Ok(crate::model::message::SearchResult {
                id: row.get(0)?,
                from_user: row.get(1)?,
                to_user: row.get(2)?,
                room: row.get(3)?,
                content: row.get(4)?,
                timestamp: row.get(5)?,
                rank: row.get(6)?,
                snippet: row.get(7)?,
            })
        };

        // Bind parameters baseado no filtro
        let results = if let Some(user) = user_filter {
            stmt.query_map(params![query, user, user, limit], map_row)?
        } else {
            stmt.query_map(params![query, limit], map_row)?
        };

        results.collect::<SqlResult<Vec<_>>>()
    }

    // ============ OPERAÇÕES DE ARQUIVOS ============

    /// Salvar metadados de arquivo
    pub fn save_file_metadata(
        &self,
        file_id: &str,
        filename: &str,
        file_size: u64,
        mime_type: &str,
        uploaded_by: &str,
        file_path: &str,
    ) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO files (file_id, filename, file_size, mime_type, uploaded_by, uploaded_at, file_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![file_id, filename, file_size as i64, mime_type, uploaded_by, now, file_path],
        )?;

        Ok(())
    }

    /// Gerar token de download temporário (válido por 1 hora)
    pub fn generate_download_token(&self, file_id: &str, for_user: &str) -> SqlResult<String> {
        let conn = self.conn.lock().unwrap();

        // Verificar se usuário tem permissão (é o uploader ou destinatário)
        let has_permission: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM files WHERE file_id = ?1 AND uploaded_by = ?2",
            params![file_id, for_user],
            |row| row.get(0),
        )?;

        if !has_permission {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        let token = uuid::Uuid::new_v4().to_string();
        let expires_at = (Utc::now() + chrono::Duration::hours(1)).to_rfc3339();

        conn.execute(
            "UPDATE files SET download_token = ?1, token_expires_at = ?2 WHERE file_id = ?3",
            params![token, expires_at, file_id],
        )?;

        Ok(token)
    }

    /// Validar token de download
    pub fn validate_download_token(&self, file_id: &str, token: &str) -> SqlResult<bool> {
        let conn = self.conn.lock().unwrap();

        let result: Option<String> = conn
            .query_row(
                "SELECT token_expires_at FROM files 
             WHERE file_id = ?1 AND download_token = ?2",
                params![file_id, token],
                |row| row.get(0),
            )
            .ok();

        if let Some(expires_at_str) = result {
            if let Ok(expires_at) = chrono::DateTime::parse_from_rfc3339(&expires_at_str) {
                return Ok(Utc::now() < expires_at.with_timezone(&Utc));
            }
        }

        Ok(false)
    }

    /// Obter informações do arquivo
    pub fn get_file_info(&self, file_id: &str) -> SqlResult<FileInfo> {
        let conn = self.conn.lock().unwrap();

        conn.query_row(
            "SELECT file_id, filename, file_size, mime_type, uploaded_by, uploaded_at, file_path
             FROM files WHERE file_id = ?1",
            params![file_id],
            |row| {
                Ok(FileInfo {
                    file_id: row.get(0)?,
                    filename: row.get(1)?,
                    file_size: row.get::<_, i64>(2)? as u64,
                    mime_type: row.get(3)?,
                    uploaded_by: row.get(4)?,
                    uploaded_at: row.get(5)?,
                    file_path: row.get(6)?,
                })
            },
        )
    }
}

/// Registro de log de moderação
#[derive(Debug, Clone)]
pub struct ModerationLog {
    pub action: String,
    pub moderator: String,
    pub target_user: String,
    pub reason: Option<String>,
    pub details: Option<String>,
    pub timestamp: String,
}

/// Informações de arquivo
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub file_id: String,
    pub filename: String,
    pub file_size: u64,
    pub mime_type: String,
    pub uploaded_by: String,
    pub uploaded_at: String,
    pub file_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_creation() {
        let db = Database::new(":memory:").unwrap();
        assert_eq!(db.count_users().unwrap(), 0);
        assert_eq!(db.count_messages().unwrap(), 0);
    }

    #[test]
    fn test_user_crud() {
        let db = Database::new(":memory:").unwrap();

        // Insert
        db.insert_user("alice", "hash123").unwrap();
        assert_eq!(db.count_users().unwrap(), 1);

        // Get
        let user = db.get_user("alice").unwrap().unwrap();
        assert_eq!(user.username, "alice");
        assert_eq!(user.password_hash, "hash123");

        // List
        let users = db.list_users().unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0], "alice");

        // Delete
        db.delete_user("alice").unwrap();
        assert_eq!(db.count_users().unwrap(), 0);
    }

    #[test]
    fn test_message_crud() {
        let db = Database::new(":memory:").unwrap();
        let now = Utc::now();

        // Insert private message
        let id = db
            .insert_message(
                "alice",
                Some("bob"),
                None,
                "Hello Bob!",
                &now,
                Some("msg-123"),
                MessageType::Private,
            )
            .unwrap();

        assert!(id > 0);
        assert_eq!(db.count_messages().unwrap(), 1);

        // Get private messages
        let messages = db.get_private_messages("alice", "bob", 10).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello Bob!");
        assert_eq!(messages[0].from_user, "alice");
    }

    #[test]
    fn test_room_messages() {
        let db = Database::new(":memory:").unwrap();
        let now = Utc::now();

        db.insert_message(
            "alice",
            None,
            Some("#general"),
            "Hello room!",
            &now,
            None,
            MessageType::Room,
        )
        .unwrap();

        let messages = db.get_room_messages("#general", 10).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].room, Some("#general".to_string()));
    }
}
