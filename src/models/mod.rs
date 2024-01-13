use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: u64,
    pub title: String,
    pub completed: bool,
}
impl Todo {
    pub fn new(id: u64, title: String) -> Self {
        Self {
            id,
            title,
            completed: false,
        }
    }
}
