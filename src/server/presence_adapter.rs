// Adapter: ChatState -> UserPresenceService
use crate::server::state::ChatState;
use messaging_domain::UserPresenceService;
use std::sync::Arc;

pub struct ChatStatePresenceAdapter {
    state: Arc<ChatState>,
}

impl ChatStatePresenceAdapter {
    pub fn new(state: Arc<ChatState>) -> Self {
        Self { state }
    }
}

impl UserPresenceService for ChatStatePresenceAdapter {
    fn is_online(&self, username: &str) -> bool {
        self.state.get_client_tx(username).is_some()
    }
}
