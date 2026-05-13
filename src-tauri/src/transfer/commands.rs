use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use crate::transfer::{generate_code, now_secs, PeerInfo, TransferSession, TransferState};

pub struct TransferAppState {
    pub sessions: Arc<Mutex<HashMap<String, TransferSession>>>,
    pub peers: Arc<Mutex<HashMap<String, PeerInfo>>>,
    pub server_port: u16,
    pub device_name: String,
}

pub fn unique_path(dir: &str, filename: &str) -> String {
    let base = dir.trim_end_matches('/');
    let path = format!("{}/{}", base, filename);
    if !std::path::Path::new(&path).exists() {
        return path;
    }
    let p = std::path::Path::new(filename);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(filename);
    let ext = p.extension().and_then(|s| s.to_str());
    for i in 1..100 {
        let name = match ext {
            Some(e) => format!("{}_{}.{}", stem, i, e),
            None => format!("{}_{}", stem, i),
        };
        let candidate = format!("{}/{}", base, name);
        if !std::path::Path::new(&candidate).exists() {
            return candidate;
        }
    }
    // All 99 numbered variants exist — use UUID suffix to guarantee uniqueness
    let p = std::path::Path::new(filename);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(filename);
    let ext = p.extension().and_then(|s| s.to_str());
    let uid = uuid::Uuid::new_v4().simple().to_string();
    let name = match ext {
        Some(e) => format!("{}_{}.{}", stem, uid, e),
        None => format!("{}_{}", stem, uid),
    };
    format!("{}/{}", base, name)
}

#[tauri::command]
pub async fn get_peers(
    state: tauri::State<'_, TransferAppState>,
) -> Result<Vec<PeerInfo>, String> {
    let peers = state.peers.lock().map_err(|e| e.to_string())?;
    Ok(peers.values().cloned().collect())
}

#[tauri::command]
pub async fn start_transfer(
    path: String,
    peer_id: String,
    state: tauri::State<'_, TransferAppState>,
) -> Result<TransferSession, String> {
    let file_path = std::path::Path::new(&path);
    if !file_path.exists() {
        return Err(format!("Path not found: {}", path));
    }
    if file_path.is_dir() {
        return Err("Directory transfer not supported in v1 — select individual files".to_string());
    }
    let filename = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid filename")?
        .to_string();
    let size = file_path.metadata().map_err(|e| e.to_string())?.len();

    let peer = {
        let peers = state.peers.lock().map_err(|e| e.to_string())?;
        peers.get(&peer_id).cloned().ok_or(format!("Peer not found: {}", peer_id))?
    };

    let session = TransferSession {
        id: uuid::Uuid::new_v4().to_string(),
        code: generate_code(),
        file_path: path.clone(),
        filename: filename.clone(),
        size,
        state: TransferState::Pending,
        sender_name: state.device_name.clone(),
        sender_addr: String::new(),
        sender_port: state.server_port,
        created_at_secs: now_secs(),
    };

    {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        sessions.insert(session.id.clone(), session.clone());
    }

    let offer_url = format!("http://{}:{}/transfer/offer", peer.addr, peer.port);
    reqwest::Client::new()
        .post(&offer_url)
        .json(&serde_json::json!({
            "session_id": session.id,
            "code": session.code,
            "filename": filename,
            "size": size,
            "sender_name": state.device_name,
            "sender_port": state.server_port,
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to reach peer: {}", e))?;

    Ok(session)
}

#[tauri::command]
pub async fn accept_transfer(
    session_id: String,
    dest_path: String,
    state: tauri::State<'_, TransferAppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let (sender_addr, sender_port, filename, size) = {
        let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
        let session = sessions
            .get_mut(&session_id)
            .ok_or("Session not found")?;
        if session.state != TransferState::Pending {
            return Err("Session is not pending".to_string());
        }
        session.state = TransferState::Accepted;
        (
            session.sender_addr.clone(),
            session.sender_port,
            session.filename.clone(),
            session.size,
        )
    };

    let sessions = state.sessions.clone();
    let sid = session_id.clone();
    tokio::spawn(async move {
        if let Err(e) = stream_file(
            StreamParams { session_id: &sid, sender_addr: &sender_addr, sender_port, dest_dir: &dest_path, filename: &filename, total: size },
            &app, &sessions,
        )
        .await
        {
            app.emit(
                "transfer-error",
                serde_json::json!({ "session_id": sid, "error": e }),
            )
            .ok();
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn reject_transfer(
    session_id: String,
    state: tauri::State<'_, TransferAppState>,
) -> Result<(), String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    if let Some(s) = sessions.get_mut(&session_id) {
        s.state = TransferState::Rejected;
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_transfer(
    session_id: String,
    state: tauri::State<'_, TransferAppState>,
) -> Result<(), String> {
    let mut sessions = state.sessions.lock().map_err(|e| e.to_string())?;
    if let Some(s) = sessions.get_mut(&session_id) {
        s.state = TransferState::Cancelled;
    }
    Ok(())
}

struct StreamParams<'a> {
    session_id: &'a str,
    sender_addr: &'a str,
    sender_port: u16,
    dest_dir: &'a str,
    filename: &'a str,
    total: u64,
}

async fn stream_file(
    p: StreamParams<'_>,
    app: &tauri::AppHandle,
    sessions: &Arc<Mutex<HashMap<String, TransferSession>>>,
) -> Result<(), String> {
    let StreamParams { session_id, sender_addr, sender_port, dest_dir, filename, total } = p;
    let url = format!(
        "http://{}:{}/transfer/{}/file",
        sender_addr, sender_port, session_id
    );

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Sender returned HTTP {}", response.status()));
    }

    let safe_filename = std::path::Path::new(filename)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("received_file");
    let dest_path = unique_path(dest_dir, safe_filename);
    let mut file = tokio::fs::File::create(&dest_path)
        .await
        .map_err(|e| format!("Cannot create file: {}", e))?;

    let mut bytes_done = 0u64;
    let mut last_emit = 0u64;

    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| format!("Write error: {}", e))?;
        bytes_done += chunk.len() as u64;
        if bytes_done.saturating_sub(last_emit) >= 256 * 1024 {
            last_emit = bytes_done;
            app.emit(
                "transfer-progress",
                serde_json::json!({
                    "session_id": session_id.to_string(),
                    "bytes_done": bytes_done,
                    "total": total,
                }),
            )
            .ok();
        }
    }

    {
        let mut sessions = sessions.lock().map_err(|e| format!("Lock failed: {}", e))?;
        if let Some(s) = sessions.get_mut(session_id) {
            s.state = TransferState::Done;
        }
    }

    app.emit(
        "transfer-complete",
        serde_json::json!({
            "session_id": session_id,
            "saved_path": dest_path,
        }),
    )
    .ok();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_path_returns_original_when_no_conflict() {
        let dir = std::env::temp_dir().to_string_lossy().to_string();
        let result = unique_path(&dir, "tracer_nonexistent_xyz_abc.txt");
        assert!(result.ends_with("tracer_nonexistent_xyz_abc.txt"));
    }

    #[test]
    fn unique_path_increments_on_conflict() {
        let dir = std::env::temp_dir().to_string_lossy().to_string();
        let filename = "tracer_test_unique_file.txt";
        let first = format!("{}/{}", dir.trim_end_matches('/'), filename);
        std::fs::write(&first, b"test").unwrap();

        let result = unique_path(&dir, filename);
        let _ = std::fs::remove_file(&first);

        assert!(result.ends_with("tracer_test_unique_file_1.txt"), "got: {}", result);
    }
}
