use std::sync::Arc;

use arsy_core::services::UserService;

/// Shared, cheaply-cloneable state injected into every handler.
#[derive(Clone)]
pub struct AppState {
    pub users: Arc<UserService>,
}

impl AppState {
    pub fn new(users: Arc<UserService>) -> Self {
        Self { users }
    }
}
