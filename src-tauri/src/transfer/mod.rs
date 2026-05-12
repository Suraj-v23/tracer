pub mod server;
pub mod discovery;
pub mod commands;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: String,
    pub name: String,
    pub addr: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransferState {
    Pending,
    Accepted,
    Done,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferSession {
    pub id: String,
    pub code: String,
    pub file_path: String,
    pub filename: String,
    pub size: u64,
    pub state: TransferState,
    pub sender_name: String,
    pub sender_addr: String,
    pub sender_port: u16,
    pub created_at_secs: u64,
}

impl TransferSession {
    pub fn is_expired(&self) -> bool {
        self.state == TransferState::Pending
            && now_secs().saturating_sub(self.created_at_secs) > 300
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn generate_code() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    (0..6)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_code_is_six_chars() {
        assert_eq!(generate_code().len(), 6);
    }

    #[test]
    fn generate_code_is_uppercase_alphanumeric() {
        for _ in 0..20 {
            let code = generate_code();
            assert!(code.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
        }
    }

    #[test]
    fn fresh_pending_session_is_not_expired() {
        let s = TransferSession {
            id: "x".into(), code: "A".into(), file_path: String::new(),
            filename: "f".into(), size: 0, state: TransferState::Pending,
            sender_name: "n".into(), sender_addr: String::new(),
            sender_port: 0, created_at_secs: now_secs(),
        };
        assert!(!s.is_expired());
    }

    #[test]
    fn old_pending_session_is_expired() {
        let s = TransferSession {
            id: "x".into(), code: "A".into(), file_path: String::new(),
            filename: "f".into(), size: 0, state: TransferState::Pending,
            sender_name: "n".into(), sender_addr: String::new(),
            sender_port: 0, created_at_secs: now_secs() - 400,
        };
        assert!(s.is_expired());
    }

    #[test]
    fn accepted_session_never_expires_even_if_old() {
        let s = TransferSession {
            id: "x".into(), code: "A".into(), file_path: String::new(),
            filename: "f".into(), size: 0, state: TransferState::Accepted,
            sender_name: "n".into(), sender_addr: String::new(),
            sender_port: 0, created_at_secs: now_secs() - 400,
        };
        assert!(!s.is_expired());
    }
}
