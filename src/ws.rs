use crate::{Client, Clients, Space, Spaces};
use futures::{FutureExt, StreamExt};
use serde::{Serialize, Deserialize};
use serde_json::{from_str};
use tokio::sync::mpsc;
use warp::ws::{Message, WebSocket};

#[derive(Serialize, Deserialize, Debug)]
pub enum UpdateRequest {
    SpaceSet { space_code : String },
    CountUpdate { mode: String, value: isize },
}

pub async fn client_connection(
    mut client: Client,
    socket: WebSocket,
    clients: Clients,
    spaces: Spaces,
) {
    // Split the websocket and create a channel for sending outgoing messages
    let client_id = client.id.clone();
    let (client_ws_sender, mut client_ws_rcv) = socket.split();
    let (client_sender, client_rcv) = mpsc::unbounded_channel();

    // Spawn a separate task to forward outgoing messages to the websocket client
    tokio::task::spawn(client_rcv.forward(client_ws_sender).map(|result| {
        if let Err(e) = result {
            eprintln!("error sending websocket msg: {}", e);
        }
    }));

    // Save a "sender" for this outgoing channel with this client, in shared memory
    client.sender = Some(client_sender);
    clients.write().await.insert(client_id.clone(), client);

    println!("{} connected", client_id);

    // Handle all messages coming from this client, until the socket is closed
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

    // Remove the client when the socket is closed
    clients.write().await.remove(&client_id);
    println!("{} disconnected", client_id);
}

async fn client_msg(msg: Message, client_id: &str, clients: &Clients, spaces: &Spaces) {
    let message = match msg.to_str() {
        Ok(v) => v,
        Err(_) => return,
    };

    // Short-circuit for pings so user-friendly clients can send these
    if message == "ping" || message == "ping\n" {
        return;
    }

    // Validate this client, and store its space code for later
    let space_code: String;
    match clients.read().await.get(client_id) {
        Some(c) => space_code = c.space_code.clone(),
        None => {
            eprintln!("no client found for id: {}", client_id);
            return;
        }
    };

    // Parse the request
    let socket_request: UpdateRequest = match from_str(&message) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error while parsing message to topics request: {}", e);
            return;
        }
    };

    // Match the request with the UpdateRequest enum
    match socket_request {

        // SpaceSet action is for joining a space
        UpdateRequest::SpaceSet{ space_code } => {

            // Lock a mutable reference to the client, and update its space_code
            if let Some(c) = clients.write().await.get_mut(client_id) {
                c.space_code = space_code;
            }
        },

        // CountUpdate action is either "relative" or "absolute" and applies to the connected client's space
        UpdateRequest::CountUpdate{ mode, value } => {
            let count: isize;

            // Block-scope this bit so that we release the space write lock ASAP
            {
                // Get the space that this client is counting for
                let mut locked = spaces.write().await;
                let space = match locked.get_mut(&space_code) {
                    Some(s) => s,
                    None => {
                        locked.insert(space_code.clone(), Space { id: space_code.clone(), count: 0 });
                        locked.get_mut(&space_code).expect("Error creating new space!")
                    }
                };
                //println!("Matched space: {} ({})", c.space_code, serde_json::to_string(&space).unwrap());

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
                .filter(|(_, c)| c.space_code == space_code)
                .for_each(|(_, c)| {
                    if let Some(sender) = &c.sender {
                        let _ = sender.send(Ok(Message::text(count.to_string())));
                    }
                });
        },
    }
}
