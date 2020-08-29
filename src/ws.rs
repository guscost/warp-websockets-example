use crate::{Client, Clients, Space, Spaces};
use futures::{FutureExt, StreamExt};
use serde::{Serialize, Deserialize};
use serde_json::{from_str};
use tokio::sync::mpsc;
use warp::ws::{Message, WebSocket};

#[derive(Serialize, Deserialize, Debug)]
pub enum UpdateRequest {
    SpaceSet { space_id : String },
    CountUpdate { mode: String, value: isize },
}

pub async fn client_connection(
    mut client: Client,
    socket: WebSocket,
    clients: Clients,
    spaces: Spaces,
) {
    let client_id = client.id.clone();
    let (client_ws_sender, mut client_ws_rcv) = socket.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();

    tokio::task::spawn(client_rcv.forward(client_ws_sender).map(|result| {
        if let Err(e) = result {
            eprintln!("error sending websocket msg: {}", e);
        }
    }));

    client.sender = Some(client_sender);
    clients.write().await.insert(client_id.clone(), client);

    println!("{} connected", client_id);

    while let Some(result) = client_ws_rcv.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("error receiving ws message for id: {}): {}", client_id, e);
                break;
            }
        };
        client_msg(msg, &client_id, &clients, &spaces).await;
    }

    clients.write().await.remove(&client_id);
    println!("{} disconnected", client_id);
}

async fn client_msg(msg: Message, client_id: &str, clients: &Clients, spaces: &Spaces) {
    let message = match msg.to_str() {
        Ok(v) => v,
        Err(_) => return,
    };

    if message == "ping" || message == "ping\n" {
        return;
    }

    let space_id: String;
    match clients.read().await.get(client_id) {
        Some(c) => space_id = c.space_id.clone(),
        None => {
            eprintln!("no client found for id: {}", client_id);
            return;
        }
    };

    let socket_request: UpdateRequest = match from_str(&message) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error while parsing message to topics request: {}", e);
            return;
        }
    };

    //println!("socket request: {}", serde_json::to_string(&socket_request).unwrap());

    match socket_request {

        // SpaceSet action is "join room" on frontend
        UpdateRequest::SpaceSet{ space_id } => {
            // Lock a mutable reference to the client, and update its space id
            if let Some(c) = clients.write().await.get_mut(client_id) {
                c.space_id = space_id;
            }
        },

        // CountUpdate action is "relative" or "absolute" and applies to the connected client's space
        UpdateRequest::CountUpdate{ mode, value } => {
            let count: isize;

            // Block-scope this bit so that we release the space write lock ASAP
            {
                // Get the space that this client is counting for
                let mut locked = spaces.write().await;
                let space = match locked.get_mut(&space_id) {
                    Some(s) => s,
                    None => {
                        locked.insert(space_id.clone(), Space { id: space_id.clone(), count: 0 });
                        locked.get_mut(&space_id).expect("Error creating new space!")
                    }
                };
                //println!("Matched space: {} ({})", c.space_id, serde_json::to_string(&space).unwrap());

                // Update the space's count
                if mode == "relative" {
                    space.count = std::cmp::max(space.count + value, 0);
                } else if mode == "absolute" {
                    space.count = std::cmp::max(value, 0);
                }

                // Set our mutable variable for broadcast
                count = space.count;
            }

            // Broadcast the count to all clients
            clients.read().await.iter()
                .filter(|(_, c)| c.space_id == space_id)
                .for_each(|(_, c)| {
                    if let Some(sender) = &c.sender {
                        let _ = sender.send(Ok(Message::text(count.to_string())));
                    }
                });
        },
    }
}
