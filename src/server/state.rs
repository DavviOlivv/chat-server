use crate::model::message::ChatMessage;
use dashmap::{DashMap, DashSet};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::sync::mpsc; // Canal para enviar mensagens para o cliente

// Tipo auxiliar para facilitar a leitura:
// Um canal que envia ChatMessage para o socket do cliente
type Tx = mpsc::Sender<ChatMessage>;

pub struct ChatState {
    // DashMap permite concorrência fina para leituras e escritas simultâneas
    // O mapa guarda: username -> (SocketAddr, Canal de Transmissão)
    pub clients: DashMap<String, (SocketAddr, Tx)>,
    // Mensagens já vistas (message_id -> instante do primeiro recebimento)
    // Usado para deduplicação de DMs reenviadas pelo cliente
    seen_message_ids: DashMap<String, Instant>,
    // Mapa A: sala -> conjunto de usernames (membros)
    pub rooms: DashMap<String, DashSet<String>>,
    // Mapa B: username -> conjunto de salas inscritas
    pub subscriptions: DashMap<String, DashSet<String>>,
    // Rate limiting básico por usuário: username -> (início da janela, contagem)
    rate_counters: DashMap<String, (Instant, u32)>,
}

impl Default for ChatState {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
            seen_message_ids: DashMap::new(),
            rooms: DashMap::new(),
            subscriptions: DashMap::new(),
            rate_counters: DashMap::new(),
        }
    }

    // Salas fixas que sempre existem no mapa `rooms`.
    fn fixed_rooms() -> Vec<&'static str> {
        vec!["#geral", "#ajuda", "#dev"]
    }

    // Inicializa as salas fixas (chamado por código externo que cria ChatState)
    // Aqui também pode ser chamado após `new()` se desejado.
    pub fn init_fixed_rooms(&self) {
        for r in Self::fixed_rooms() {
            self.rooms.entry(r.to_string()).or_default();
        }
    }

    /// Valida username: alfanumérico + underscore/hífen, comprimento 3-20 caracteres.
    /// Retorna Ok(()) se válido, Err(mensagem) se inválido.
    pub fn validate_username(username: &str) -> Result<(), String> {
        // Comprimento
        if username.len() < 3 {
            return Err("Username deve ter no mínimo 3 caracteres".to_string());
        }
        if username.len() > 20 {
            return Err("Username deve ter no máximo 20 caracteres".to_string());
        }

        // Regex: ASCII alfanumérico + underscore + hífen
        let valid_chars = username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !valid_chars {
            return Err(
                "Username só pode conter letras ASCII, números, underscore (_) e hífen (-)"
                    .to_string(),
            );
        }

        Ok(())
    }

    /// Verifica se o username já está em uso.
    pub fn is_username_taken(&self, username: &str) -> bool {
        self.clients.contains_key(username)
    }

    /// Checa rate limit básico (limite de mensagens por segundo) para um username.
    /// Retorna true se a mensagem pode ser processada, false se excedeu o limite.
    pub fn check_rate_limit(&self, username: &str, limit_per_sec: u32) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(1);

        if let Some(mut entry) = self.rate_counters.get_mut(username) {
            let (ref mut start, ref mut count) = *entry;
            if now.duration_since(*start) >= window {
                // Nova janela
                *start = now;
                *count = 1;
                true
            } else if *count < limit_per_sec {
                *count += 1;
                true
            } else {
                // Estourou o limite na janela atual
                false
            }
        } else {
            // Primeiro evento para este usuário
            self.rate_counters.insert(username.to_string(), (now, 1));
            true
        }
    }

    // Função para registrar um novo usuário
    pub fn add_client(&self, addr: SocketAddr, username: String, tx: Tx) {
        self.clients.insert(username, (addr, tx));
    }

    // Função para contar número de clientes online
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    // Função para remover um usuário
    pub fn remove_client(&self, addr: &SocketAddr) -> Option<String> {
        // Evite deadlocks no DashMap: iterar com escopo curto e clonar o minimo necessário
        let mut found: Option<String> = None;
        {
            for entry in self.clients.iter() {
                if entry.value().0 == *addr {
                    found = Some(entry.key().clone());
                    break;
                }
            }
        } // entry iterator guard dropped aqui
        if let Some(username) = found {
            self.remove_client_by_username(&username);
            Some(username)
        } else {
            None
        }
    }

    // Função para remover um usuário pelo username (para comandos admin)
    pub fn remove_client_by_username(&self, username: &str) {
        // Primeiro, limpar as inscrições do usuário (Mapa B) e remover do membros (Mapa A)
        if let Some(subs_ref) = self.subscriptions.remove(username) {
            let subs_set = subs_ref.1; // DashSet<String>
                                       // Colete salas para iterar
            let rooms_vec: Vec<String> = subs_set.iter().map(|r| r.key().clone()).collect();
            for room in rooms_vec {
                if let Some(room_entry) = self.rooms.get(&room) {
                    room_entry.remove(username);
                    // Se sala está vazia e não é fixa, remover a chave
                    let empty = room_entry.is_empty();
                    drop(room_entry);
                    if empty && !Self::fixed_rooms().contains(&room.as_str()) {
                        self.rooms.remove(&room);
                    }
                }
            }
        }

        // Finalmente remover cliente do mapa de clients e dos contadores de rate limit
        self.rate_counters.remove(username);
        self.clients.remove(username);
    }

    // Função auxiliar para verificar se usuário existe
    pub fn has_user(&self, username: &str) -> bool {
        self.clients.contains_key(username)
    }

    /// Faz o usuário entrar na sala especificada. Cria a sala se necessário.
    pub fn join_room(&self, username: &str, room: &str) {
        // Garantir que a sala exista (criação dinâmica)
        let entry = self
            .rooms
            .entry(room.to_string())
            .or_default();
        entry.insert(username.to_string());

        // Adicionar a sala nas inscrições do usuário
        let user_entry = self
            .subscriptions
            .entry(username.to_string())
            .or_default();
        user_entry.insert(room.to_string());
    }

    /// Faz o usuário sair da sala. Aplica GC se a sala estiver vazia e não for fixa.
    pub fn leave_room(&self, username: &str, room: &str) {
        if let Some(room_entry) = self.rooms.get(room) {
            room_entry.remove(username);
            let empty = room_entry.is_empty();
            drop(room_entry);
            if empty && !Self::fixed_rooms().contains(&room) {
                self.rooms.remove(room);
            }
        }

        if let Some(subs_entry) = self.subscriptions.get(username) {
            subs_entry.remove(room);
            // If user's subscription set becomes empty, remove the entry
            if subs_entry.is_empty() {
                drop(subs_entry);
                self.subscriptions.remove(username);
            }
        }
    }

    // Busca o canal (`tx`) associado a um username para envio de DM
    pub fn get_client_tx(&self, target_username: &str) -> Option<Tx> {
        // Acesso direto por chave (mais eficiente)
        self.clients
            .get(target_username)
            .map(|r| r.value().1.clone())
    }

    // Retorna a lista de usernames online
    pub fn list_usernames(&self) -> Vec<String> {
        self.clients
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Retorna o `username` associado a um `SocketAddr`, se existir.
    pub fn username_by_addr(&self, addr: &SocketAddr) -> Option<String> {
        // iterar com escopo curto para evitar manter guards do DashMap
        let mut found: Option<String> = None;
        {
            for entry in self.clients.iter() {
                if entry.value().0 == *addr {
                    found = Some(entry.key().clone());
                    break;
                }
            }
        }
        found
    }

    /// Verifica se um `message_id` já foi visto. Se não, marca como visto.
    /// Retorna `true` se já foi visto (duplicata), `false` caso contrário.
    /// Realiza limpeza simples de entradas antigas para evitar crescimento ilimitado.
    pub fn check_and_mark_message_id(&self, id: &str) -> bool {
        // Limpeza: remove entradas com mais de 1 hora
        let now = Instant::now();
        let expiry = Duration::from_secs(60 * 60);

        // Coleta chaves expiradas e remove-as (operações seguras e concorrentes)
        let mut to_remove = Vec::new();
        for entry in self.seen_message_ids.iter() {
            if now.duration_since(*entry.value()) >= expiry {
                to_remove.push(entry.key().clone());
            }
        }
        for k in to_remove {
            self.seen_message_ids.remove(&k);
        }

        if self.seen_message_ids.contains_key(id) {
            true
        } else {
            self.seen_message_ids.insert(id.to_string(), now);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::core::ChatCore;
    use std::net::SocketAddr;
    use tokio::sync::mpsc;

    #[test]
    fn test_validate_username() {
        // Válidos
        assert!(ChatState::validate_username("user123").is_ok());
        assert!(ChatState::validate_username("alice_bob").is_ok());
        assert!(ChatState::validate_username("test-user").is_ok());
        assert!(ChatState::validate_username("a_b-c123").is_ok());
        assert!(ChatState::validate_username("abc").is_ok()); // mínimo 3

        // Muito curto
        assert!(ChatState::validate_username("ab").is_err());
        assert!(ChatState::validate_username("a").is_err());
        assert!(ChatState::validate_username("").is_err());

        // Muito longo
        assert!(ChatState::validate_username("usuario_com_nome_muito_longo_demais").is_err());
        assert!(ChatState::validate_username("123456789012345678901").is_err()); // 21 chars

        // Caracteres inválidos
        assert!(ChatState::validate_username("user@test").is_err());
        assert!(ChatState::validate_username("user.name").is_err());
        assert!(ChatState::validate_username("user name").is_err());
        assert!(ChatState::validate_username("joão").is_err());
        assert!(ChatState::validate_username("user#123").is_err());
        assert!(ChatState::validate_username("user$").is_err());
    }

    #[test]
    fn test_username_uniqueness() {
        let state = ChatState::new();
        let (tx1, _rx1) = mpsc::channel(1000);
        let (tx2, _rx2) = mpsc::channel(1000);

        let addr1: SocketAddr = "127.0.0.1:19000".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:19001".parse().unwrap();

        // Primeiro usuário
        assert!(!state.is_username_taken("alice"));
        state.add_client(addr1, "alice".to_string(), tx1);
        assert!(state.is_username_taken("alice"));

        // Segundo usuário com nome diferente
        assert!(!state.is_username_taken("bob"));
        state.add_client(addr2, "bob".to_string(), tx2);
        assert!(state.is_username_taken("bob"));

        // Tentar verificar nome já em uso
        assert!(state.is_username_taken("alice"));
    }

    #[test]
    fn test_rate_limit_basic_window() {
        let state = ChatState::new();
        let user = "ratelimited";

        // Primeiras 10 mensagens na mesma janela de 1s devem ser permitidas
        for i in 0..10 {
            assert!(state.check_rate_limit(user, 10), "falhou na iteracao {}", i);
        }

        // 11ª mensagem na mesma janela deve ser bloqueada
        assert!(!state.check_rate_limit(user, 10));
    }

    #[test]
    fn test_add_get_list_remove_clients() {
        let state = ChatState::new();

        let (tx1, mut rx1) = mpsc::channel(1000);
        let (tx2, _rx2) = mpsc::channel(1000);

        let addr1: SocketAddr = "127.0.0.1:20000".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:20001".parse().unwrap();

        state.add_client(addr1, "user1".to_string(), tx1);
        state.add_client(addr2, "user2".to_string(), tx2);

        // list_usernames contém ambos (ordem não garantida)
        let mut users = state.list_usernames();
        users.sort();
        assert_eq!(users, vec!["user1".to_string(), "user2".to_string()]);

        // get_client_tx encontra o canal e podemos enviar mensagem
        let opt = state.get_client_tx("user1");
        assert!(opt.is_some());
        let tx = opt.unwrap();
        let _ = tx.try_send(crate::model::message::ChatMessage::Text {
            from: "srv".into(),
            content: "hi".into(),
            timestamp: chrono::Utc::now(),
        });
        match rx1.try_recv() {
            Ok(msg) => match msg {
                crate::model::message::ChatMessage::Text { from, content, .. } => {
                    assert_eq!(from, "srv");
                    assert_eq!(content, "hi");
                }
                _ => panic!("unexpected message"),
            },
            Err(e) => panic!("didn't receive: {:?}", e),
        }

        // remove_client retorna username e atualiza a lista
        let removed = state.remove_client(&addr1).expect("should remove");
        assert_eq!(removed, "user1".to_string());
        let users2 = state.list_usernames();
        assert_eq!(users2, vec!["user2".to_string()]);
    }

    #[test]
    fn test_join_leave_and_gc() {
        let state = ChatState::new();
        state.init_fixed_rooms();

        // user joins ephemeral room
        state.join_room("alice", "#rust");
        assert!(state.rooms.contains_key("#rust"));
        // alice leaves, room should be removed (ephemeral)
        state.leave_room("alice", "#rust");
        assert!(!state.rooms.contains_key("#rust"));

        // fixed room remains even se ficar vazia
        state.join_room("bob", "#geral");
        assert!(state.rooms.contains_key("#geral"));
        state.leave_room("bob", "#geral");
        // fixed room must remain present (maybe vazio)
        assert!(state.rooms.contains_key("#geral"));
    }

    #[test]
    fn test_remove_client_cleans_up() {
        let state = ChatState::new();
        state.init_fixed_rooms();

        let (tx_a, _rx_a) = mpsc::channel(1000);
        let (tx_b, _rx_b) = mpsc::channel(1000);

        let addr_a: SocketAddr = "127.0.0.1:30000".parse().unwrap();
        let addr_b: SocketAddr = "127.0.0.1:30001".parse().unwrap();

        state.add_client(addr_a, "davi".to_string(), tx_a);
        state.add_client(addr_b, "x".to_string(), tx_b);

        // join multiple rooms
        state.join_room("davi", "#r1");
        state.join_room("davi", "#r2");
        state.join_room("x", "#r2");

        // ensure presence
        assert!(state.clients.contains_key("davi"));
        assert!(state.rooms.get("#r1").unwrap().contains("davi"));
        assert!(state.rooms.get("#r2").unwrap().contains("davi"));
        assert!(state.subscriptions.contains_key("davi"));

        // remove by addr
        let removed = state.remove_client(&addr_a);
        assert_eq!(removed, Some("davi".to_string()));

        // client no longer in clients
        assert!(!state.clients.contains_key("davi"));
        // subscriptions should be removed
        assert!(!state.subscriptions.contains_key("davi"));
        // should not be member in rooms
        if let Some(r2) = state.rooms.get("#r2") {
            assert!(!r2.contains("davi"));
        };
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    #[ignore]
    async fn stress_test_concurrent_joins_and_broadcast() {
        use std::sync::Arc as StdArc;
        use tokio::task;

        let state = StdArc::new(ChatState::new());
        state.init_fixed_rooms();

        let room = "#rust-heavy".to_string();
        // NOTE: defaults are small for CI; override via env for heavier local runs
        let n: usize = std::env::var("STRESS_N")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8usize);
        let iterations: usize = std::env::var("STRESS_ITER")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50usize);

        // Prepare clients with addresses and channels
        let mut addrs = Vec::new();
        let mut txs = Vec::new();
        for i in 0..n {
            let (tx, _rx) = mpsc::channel(1000);
            let port = 40000 + i as u16;
            let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
            let username = format!("user{}", i);
            state.add_client(addr, username.clone(), tx.clone());
            addrs.push((addr, username));
            txs.push(tx);
        }

        let core = StdArc::new(ChatCore::new(state.clone()));

        // Barrier to release all tasks at once
        let barrier = StdArc::new(tokio::sync::Barrier::new(n + 1));

        // Spawn churn tasks
        let mut handles = Vec::new();
        for i in 0..n {
            let state_c = state.clone();
            let barrier_c = barrier.clone();
            let username = addrs[i].1.clone();
            let _addr = addrs[i].0;
            let _tx_clone = txs[i].clone();
            let room_i = room.clone();

            let h = task::spawn(async move {
                // wait for the common start
                barrier_c.wait().await;

                for j in 0..iterations {
                    if i % 2 == 0 {
                        state_c.join_room(&username, &room_i);
                    } else {
                        state_c.leave_room(&username, &room_i);
                    }

                    // light churn; avoid heavy remove/re-add in unit test
                    if j % 250 == 0 {
                        // no-op at this scale
                    }
                }
            });
            handles.push(h);
        }

        // Broadcaster task
        let core_b = core.clone();
        let barrier_b = barrier.clone();
        let addrs_clone = addrs.clone();
        let room_clone = room.clone();
        let broadcaster = task::spawn(async move {
            barrier_b.wait().await;
            // while churn happening, send messages
            for k in 0..(iterations / 10) {
                // pick a sender (user0)
                let sender_addr = addrs_clone[0].0;
                let msg = crate::model::message::ChatMessage::RoomText {
                    from: "user0".into(),
                    room: room_clone.clone(),
                    content: format!("ping{}", k),
                    timestamp: chrono::Utc::now(),
                };
                // use core to handle (it will validate membership)
                core_b.handle_message(msg, sender_addr);
                // small yield
                tokio::task::yield_now().await;
            }
        });

        // Release all tasks
        barrier.wait().await;

        // Wait for churn tasks to finish
        for h in handles {
            let _ = h.await;
        }
        // Wait for broadcaster
        let _ = broadcaster.await;

        // Final cleanup: ensure everyone leaves
        for (_addr, username) in addrs.iter() {
            state.leave_room(username, &room);
            // ensure subscriptions cleared
            if state.subscriptions.contains_key(username) {
                state.subscriptions.remove(username);
            }
        }

        // Room should be gone after all left
        assert!(!state.rooms.contains_key(&room));
    }
}
