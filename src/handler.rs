use crate::{ws, Client, Clients, Spaces, Result};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use warp::{http::StatusCode, reply::json, Reply};

#[derive(Deserialize, Debug)]
pub struct RegisterRequest {
    space_code: String,
}

#[derive(Serialize, Debug)]
pub struct RegisterResponse {
    url: String,
}

#[derive(Serialize, Debug)]
pub struct RegisterError {
    message: String,
}

// Valid space codes are 4-6 upper-case letters
lazy_static! {
    static ref SPACE_CODE_REGEX: Regex = Regex::new(r"^[A-Z]{4,6}$").unwrap();
}

// GET /health
pub async fn health_handler() -> Result<impl Reply> {
    Ok(StatusCode::OK)
}

// POST /register
pub async fn register_handler(body: RegisterRequest, clients: Clients) -> Result<impl Reply> {
    
    // Only allow valid space codes
    if !SPACE_CODE_REGEX.is_match(&body.space_code) {
        return Err(warp::reject::not_found());
    }

    // Generate a new client id and save this space code
    let uuid = Uuid::new_v4().simple().to_string();
    clients.write().await.insert(
        uuid.clone(),
        Client {
            id: uuid.clone(),
            space_code: body.space_code,
            sender: None,
        },
    );

    // All set up, return the client id in a URL for connecting to WS
    Ok(json(&RegisterResponse {
        url: format!("ws://0.0.0.0:8000/ws/{}", uuid),
    }))
}

// DELETE /register
pub async fn unregister_handler(client_id: String, clients: Clients) -> Result<impl Reply> {
    clients.write().await.remove(&client_id);
    Ok(StatusCode::OK)
}

// WS message handler 
pub async fn ws_handler(ws: warp::ws::Ws, client_id: String, clients: Clients, spaces: Spaces) -> Result<impl Reply> {
    let client = clients.read().await.get(&client_id).cloned();
    match client {
        Some(c) => Ok(ws.on_upgrade(move |socket| ws::client_connection(c, socket, clients, spaces))),
        None => Err(warp::reject::not_found()),
    }
}
