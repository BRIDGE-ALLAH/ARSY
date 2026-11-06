use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A registered user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub name: String,
    pub email: String,
}

/// Payload accepted when creating a user, before validation and id assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewUser {
    pub name: String,
    pub email: String,
}
