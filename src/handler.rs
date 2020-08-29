use crate::{ws, Client, Clients, Spaces, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use warp::{http::StatusCode, reply::json, Reply};

#[derive(Deserialize, Debug)]
pub struct RegisterRequest {
}

#[derive(Serialize, Debug)]
pub struct RegisterResponse {
    url: String,
}



pub async fn register_handler(_body: RegisterRequest, clients: Clients) -> Result<impl Reply> {
    let uuid = Uuid::new_v4().simple().to_string();
    register_client(uuid.clone(), clients).await;
    Ok(json(&RegisterResponse {
        url: format!("ws://192.168.188.114:8000/ws/{}", uuid),
    }))
}

async fn register_client(client_id: String, clients: Clients) {
    clients.write().await.insert(
        client_id.clone(),
        Client {
            id: client_id,
            space_id: String::from("QZXP"),
            sender: None,
        },
    );
}

pub async fn unregister_handler(client_id: String, clients: Clients) -> Result<impl Reply> {
    clients.write().await.remove(&client_id);
    Ok(StatusCode::OK)
}

pub async fn ws_handler(ws: warp::ws::Ws, client_id: String, clients: Clients, spaces: Spaces) -> Result<impl Reply> {
    let client = clients.read().await.get(&client_id).cloned();
    match client {
        Some(c) => Ok(ws.on_upgrade(move |socket| ws::client_connection(c, socket, clients, spaces))),
        None => Err(warp::reject::not_found()),
    }
}

pub async fn health_handler() -> Result<impl Reply> {
    Ok(StatusCode::OK)
}
