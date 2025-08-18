// use percent_encoding::percent_encode;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

const FRAGMENT: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');

// Use warp's Message type consistently
pub type WsClients = Arc<RwLock<broadcast::Sender<warp::ws::Message>>>;

pub fn create_ws_state() -> WsClients {
    let (tx, _rx) = broadcast::channel(100);
    Arc::new(RwLock::new(tx))
}

pub async fn broadcast_new_media(clients: &WsClients) {
    let message_json = json!({
        "event": "browser_backend",
        "url": "/?ws=true"
    });

    let message_string = message_json.to_string();
    let ws_message = warp::ws::Message::text(message_string);

    // Get the sender and send message
    let sender = clients.read().await; // This returns a guard, not a Result
    let _ = sender.send(ws_message);
}

pub async fn broadcast_new_song(clients: &WsClients, uri: String) {
    let encoded_uri = utf8_percent_encode(&uri, FRAGMENT).to_string();
    let message_json = json!({
        "event": "song",
        "url": format!("/sounds/{}?ws=true", encoded_uri)
    });

    let message_string = message_json.to_string();
    let ws_message = warp::ws::Message::text(message_string);

    // Get the sender and send message
    let sender = clients.read().await; // This returns a guard, not a Result
    let _ = sender.send(ws_message);
}

pub async fn broadcast_new_browser_raw(clients: &WsClients, url: String) {
    let message_json = json!({
        "event": "browser_raw",
        "url": url,
    });

    let message_string = message_json.to_string();
    let ws_message = warp::ws::Message::text(message_string);

    // Get the sender and send message
    let sender = clients.read().await; // This returns a guard, not a Result
    let _ = sender.send(ws_message);
}

// WebSocket connection handler
use futures_util::{SinkExt, StreamExt};

pub async fn ws_handler(
    ws: warp::ws::Ws,
    clients: WsClients,
) -> Result<impl warp::Reply, warp::Rejection> {
    Ok(ws.on_upgrade(move |websocket| handle_websocket(websocket, clients)))
}

async fn handle_websocket(websocket: warp::ws::WebSocket, clients: WsClients) {
    let (mut ws_sender, mut ws_receiver) = websocket.split();

    // Subscribe to broadcast channel
    let mut rx = {
        let sender = clients.read().await; // Await the future
        sender.subscribe()
    };

    // Handle incoming messages (keepalive/pong)
    let incoming_task = tokio::spawn(async move {
        while let Some(result) = ws_receiver.next().await {
            match result {
                Ok(msg) if msg.is_pong() => {}      // Handle pong messages
                Ok(msg) if msg.is_close() => break, // Handle close messages
                Err(_) => break,
                _ => {}
            }
        }
    });

    // Handle outgoing messages (broadcast)
    let outgoing_task = tokio::spawn(async move {
        while let Ok(message) = rx.recv().await {
            if ws_sender.send(message).await.is_err() {
                break;
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = incoming_task => {},
        _ = outgoing_task => {},
    }
}
