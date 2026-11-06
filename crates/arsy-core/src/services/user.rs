use std::sync::Arc;

use uuid::Uuid;

use crate::error::CoreError;
use crate::models::{NewUser, User};

/// Storage port for [`User`] records. Implemented by adapters in `arsy-infra`
/// so the domain layer stays free of I/O.
pub trait UserRepository: Send + Sync {
    fn list(&self) -> Result<Vec<User>, CoreError>;
    fn get(&self, id: Uuid) -> Result<User, CoreError>;
    fn insert(&self, user: User) -> Result<User, CoreError>;
}

/// Business rules around user accounts, on top of any [`UserRepository`].
pub struct UserService {
    repo: Arc<dyn UserRepository>,
}

impl UserService {
    pub fn new(repo: Arc<dyn UserRepository>) -> Self {
        Self { repo }
    }

    pub fn list(&self) -> Result<Vec<User>, CoreError> {
        self.repo.list()
    }

    pub fn get(&self, id: Uuid) -> Result<User, CoreError> {
        self.repo.get(id)
    }

    /// Validates the input, rejects duplicate emails, then persists the user.
    pub fn create(&self, input: NewUser) -> Result<User, CoreError> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(CoreError::InvalidInput("name must not be empty".into()));
        }
        let email = input.email.trim();
        if !email.contains('@') || email.starts_with('@') || email.ends_with('@') {
            return Err(CoreError::InvalidInput(format!("invalid email: {email}")));
        }
        let existing = self.repo.list()?;
        if existing.iter().any(|u| u.email.eq_ignore_ascii_case(email)) {
            return Err(CoreError::InvalidInput(format!(
                "email already registered: {email}"
            )));
        }
        let user = User {
            id: Uuid::new_v4(),
            name: name.to_string(),
            email: email.to_lowercase(),
        };
        self.repo.insert(user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Minimal fake: keeps everything in memory, no duplicate rejection.
    #[derive(Default)]
    struct FakeRepo(Mutex<HashMap<Uuid, User>>);

    impl UserRepository for FakeRepo {
        fn list(&self) -> Result<Vec<User>, CoreError> {
            Ok(self.0.lock().unwrap().values().cloned().collect())
        }
        fn get(&self, id: Uuid) -> Result<User, CoreError> {
            self.0
                .lock()
                .unwrap()
                .get(&id)
                .cloned()
                .ok_or_else(|| CoreError::NotFound(format!("user {id}")))
        }
        fn insert(&self, user: User) -> Result<User, CoreError> {
            self.0.lock().unwrap().insert(user.id, user.clone());
            Ok(user)
        }
    }

    fn service() -> UserService {
        UserService::new(Arc::new(FakeRepo::default()))
    }

    #[test]
    fn create_trims_name_and_lowercases_email() {
        let created = service()
            .create(NewUser {
                name: "  Ada Lovelace ".into(),
                email: " ADA@example.com ".into(),
            })
            .unwrap();
        assert_eq!(created.name, "Ada Lovelace");
        assert_eq!(created.email, "ada@example.com");
    }

    #[test]
    fn create_rejects_blank_name() {
        let err = service()
            .create(NewUser {
                name: "   ".into(),
                email: "ada@example.com".into(),
            })
            .unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }

    #[test]
    fn create_rejects_invalid_email() {
        for email in ["not-an-email", "@example.com", "ada@"] {
            let err = service()
                .create(NewUser {
                    name: "Ada".into(),
                    email: email.into(),
                })
                .unwrap_err();
            assert!(matches!(err, CoreError::InvalidInput(_)), "{email}");
        }
    }

    #[test]
    fn create_rejects_duplicate_email_case_insensitively() {
        let svc = service();
        svc.create(NewUser {
            name: "Ada".into(),
            email: "ada@example.com".into(),
        })
        .unwrap();
        let err = svc
            .create(NewUser {
                name: "Ada2".into(),
                email: "ADA@example.com".into(),
            })
            .unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }
}
