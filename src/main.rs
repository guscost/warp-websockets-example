use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use serde::{Serialize};
use tokio::sync::{mpsc, RwLock};
use warp::{ws::Message, Filter, Rejection};

mod handler;
mod ws;

type Result<T> = std::result::Result<T, Rejection>;
type Clients = Arc<RwLock<HashMap<String, Client>>>;
type Spaces = Arc<RwLock<HashMap<String, Space>>>;

#[derive(Debug, Clone)]
pub struct Client {
    pub id: String,
    pub space_code: String,
    pub sender: Option<mpsc::UnboundedSender<std::result::Result<Message, warp::Error>>>,
}

#[derive(Serialize, Debug, Clone)]
pub struct Space {
    pub id: String,
    pub count: isize,
}

fn with_spaces(spaces: Spaces) -> impl Filter<Extract = (Spaces,), Error = Infallible> + Clone {
    warp::any().map(move || spaces.clone())
}
fn with_clients(clients: Clients) -> impl Filter<Extract = (Clients,), Error = Infallible> + Clone {
    warp::any().map(move || clients.clone())
}

#[tokio::main]
async fn main() {
    let clients: Clients = Arc::new(RwLock::new(HashMap::new()));
    let spaces: Spaces = Arc::new(RwLock::new(HashMap::new()));

    let health_route = warp::path!("health")
        .and(warp::get())
        .and_then(handler::health_handler);

    let register = warp::path("register");
    let register_routes = register
        .and(warp::post())
        .and(warp::body::json())
        .and(with_clients(clients.clone()))
        .and_then(handler::register_handler)
        .or(register
            .and(warp::delete())
            .and(warp::path::param())
            .and(with_clients(clients.clone()))
            .and_then(handler::unregister_handler));

    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(warp::path::param())
        .and(with_clients(clients.clone()))
        .and(with_spaces(spaces.clone()))
        .and_then(handler::ws_handler);

    let routes = health_route
        .or(register_routes)
        .or(ws_route)
        .with(warp::cors().allow_any_origin());

    warp::serve(routes).run(([0, 0, 0, 0], 8000)).await;
}
