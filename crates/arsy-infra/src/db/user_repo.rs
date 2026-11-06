use std::collections::HashMap;
use std::sync::Mutex;

use arsy_core::error::CoreError;
use arsy_core::models::User;
use arsy_core::services::UserRepository;
use uuid::Uuid;

/// Development and test adapter: keeps users in process memory.
#[derive(Default)]
pub struct InMemoryUserRepository {
    users: Mutex<HashMap<Uuid, User>>,
}

impl UserRepository for InMemoryUserRepository {
    fn list(&self) -> Result<Vec<User>, CoreError> {
        let guard = self
            .users
            .lock()
            .map_err(|e| CoreError::Internal(format!("users lock poisoned: {e}")))?;
        // Sorted for deterministic output across calls.
        let mut users: Vec<User> = guard.values().cloned().collect();
        users.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        Ok(users)
    }

    fn get(&self, id: Uuid) -> Result<User, CoreError> {
        let guard = self
            .users
            .lock()
            .map_err(|e| CoreError::Internal(format!("users lock poisoned: {e}")))?;
        guard
            .get(&id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound(format!("user {id}")))
    }

    fn insert(&self, user: User) -> Result<User, CoreError> {
        let mut guard = self
            .users
            .lock()
            .map_err(|e| CoreError::Internal(format!("users lock poisoned: {e}")))?;
        guard.insert(user.id, user.clone());
        Ok(user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: Uuid, name: &str) -> User {
        User {
            id,
            name: name.to_string(),
            email: format!("{name}@example.com"),
        }
    }

    #[test]
    fn insert_then_get_roundtrips() {
        let repo = InMemoryUserRepository::default();
        let id = Uuid::new_v4();
        repo.insert(user(id, "ada")).unwrap();
        assert_eq!(repo.get(id).unwrap().name, "ada");
    }

    #[test]
    fn get_missing_returns_not_found() {
        let repo = InMemoryUserRepository::default();
        let err = repo.get(Uuid::new_v4()).unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)));
    }

    #[test]
    fn list_is_sorted_by_name() {
        let repo = InMemoryUserRepository::default();
        repo.insert(user(Uuid::new_v4(), "grace")).unwrap();
        repo.insert(user(Uuid::new_v4(), "ada")).unwrap();
        let names: Vec<String> = repo.list().unwrap().into_iter().map(|u| u.name).collect();
        assert_eq!(names, ["ada", "grace"]);
    }
}
