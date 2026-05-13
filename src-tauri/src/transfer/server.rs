use axum::{
    body::Body,
    extract::{ConnectInfo, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use tokio_util::io::ReaderStream;
use crate::transfer::{now_secs, TransferSession, TransferState};

#[derive(Clone)]
pub struct ServerState {
    pub sessions: Arc<Mutex<HashMap<String, TransferSession>>>,
    pub app_handle: tauri::AppHandle,
}

#[derive(Deserialize)]
pub struct OfferPayload {
    pub session_id: String,
    pub code: String,
    pub filename: String,
    pub size: u64,
    pub sender_name: String,
    pub sender_port: u16,
}

pub async fn start_server(
    sessions: Arc<Mutex<HashMap<String, TransferSession>>>,
    app_handle: tauri::AppHandle,
    _device_name: String,
) -> u16 {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
        .await
        .expect("Failed to bind transfer server");
    let port = listener.local_addr().unwrap().port();

    let state = ServerState { sessions, app_handle };
    let app = Router::new()
        .route("/transfer/offer", post(handle_offer))
        .route("/transfer/:session_id/file", get(handle_file))
        .with_state(state)
        .into_make_service_with_connect_info::<SocketAddr>();

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("Transfer server crashed");
    });

    port
}

async fn handle_offer(
    State(s): State<ServerState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    Json(payload): Json<OfferPayload>,
) -> StatusCode {
    let session = TransferSession {
        id: payload.session_id.clone(),
        code: payload.code.clone(),
        file_path: String::new(),
        filename: payload.filename.clone(),
        size: payload.size,
        state: TransferState::Pending,
        sender_name: payload.sender_name.clone(),
        sender_addr: remote.ip().to_string(),
        sender_port: payload.sender_port,
        created_at_secs: now_secs(),
    };

    {
        let mut sessions = s.sessions.lock().unwrap();
        sessions.retain(|_, v| !v.is_expired());
        if sessions.contains_key(&payload.session_id) {
            return StatusCode::CONFLICT;
        }
        sessions.insert(payload.session_id.clone(), session);
    }

    s.app_handle
        .emit(
            "incoming-transfer",
            serde_json::json!({
                "session_id": payload.session_id,
                "code": payload.code,
                "filename": payload.filename,
                "size": payload.size,
                "sender_name": payload.sender_name,
            }),
        )
        .ok();

    StatusCode::OK
}

async fn handle_file(
    State(s): State<ServerState>,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let file_path = {
        let mut sessions = s.sessions.lock().unwrap();
        let session = sessions.get_mut(&session_id).ok_or(StatusCode::NOT_FOUND)?;
        match session.state {
            TransferState::Pending => {
                session.state = TransferState::Accepted;
            }
            TransferState::Accepted => {}
            _ => return Err(StatusCode::FORBIDDEN),
        }
        session.file_path.clone()
    };

    let file = tokio::fs::File::open(&file_path)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let stream = ReaderStream::new(file);
    Ok((StatusCode::OK, Body::from_stream(stream)))
}
