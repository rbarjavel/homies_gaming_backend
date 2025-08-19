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
    tracing::info!("Created WebSocket broadcast channel with capacity 100");
    Arc::new(RwLock::new(tx))
}

pub async fn broadcast_new_media(clients: &WsClients) {
    tracing::info!("Broadcasting new media event");
    let message_json = json!({
        "event": "browser_backend",
        "url": "/?ws=true"
    });

    let message_string = message_json.to_string();
    let ws_message = warp::ws::Message::text(message_string);

    // Get the sender and send message
    let sender = clients.read().await; // This returns a guard, not a Result
    let result = sender.send(ws_message);
    tracing::info!("Broadcast new media result: {:?}", result);
}

pub async fn broadcast_new_song(clients: &WsClients, uri: String) {
    tracing::info!("Broadcasting new song event: {}", uri);
    let encoded_uri = utf8_percent_encode(&uri, FRAGMENT).to_string();
    let message_json = json!({
        "event": "song",
        "url": format!("/sounds/{}?ws=true", encoded_uri)
    });

    let message_string = message_json.to_string();
    let ws_message = warp::ws::Message::text(message_string);

    // Get the sender and send message
    let sender = clients.read().await; // This returns a guard, not a Result
    let result = sender.send(ws_message);
    tracing::info!("Broadcast new song result: {:?}", result);
}

pub async fn broadcast_new_browser_raw(clients: &WsClients, url: String) {
    tracing::info!("Broadcasting new browser raw event: {}", url);
    let message_json = json!({
        "event": "browser_raw",
        "url": url,
    });

    let message_string = message_json.to_string();
    let ws_message = warp::ws::Message::text(message_string);

    // Get the sender and send message
    let sender = clients.read().await; // This returns a guard, not a Result
    let result = sender.send(ws_message);
    tracing::info!("Broadcast new browser raw result: {:?}", result);
}

pub async fn broadcast_video_event(clients: &WsClients, filename: String) {
    let video_url = format!("/uploads/{}", filename);
    tracing::info!("Broadcasting video event for: {}", video_url);
    let message_json = json!({
        "event": "video",
        "url": video_url
    });

    let message_string = message_json.to_string();
    let ws_message = warp::ws::Message::text(message_string);

    // Get the sender and send message
    let sender = clients.read().await;
    let result = sender.send(ws_message);
    tracing::info!("Broadcast video event result: {:?}", result);

    tracing::info!("Broadcasted video event for: {}", video_url);
}

// WebSocket connection handler
use futures_util::{SinkExt, StreamExt};

pub async fn ws_handler(
    ws: warp::ws::Ws,
    clients: WsClients,
) -> Result<impl warp::Reply, warp::Rejection> {
    tracing::info!("WebSocket connection request received");
    Ok(ws.on_upgrade(move |websocket| handle_websocket(websocket, clients)))
}

async fn handle_websocket(websocket: warp::ws::WebSocket, clients: WsClients) {
    tracing::info!("Handling new WebSocket connection");
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
                Ok(msg) if msg.is_pong() => {
                    tracing::debug!("Received pong message");
                }      // Handle pong messages
                Ok(msg) if msg.is_close() => {
                    tracing::info!("Received close message, closing connection");
                    break;
                } // Handle close messages
                Err(e) => {
                    tracing::warn!("WebSocket receive error: {:?}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Handle outgoing messages (broadcast)
    let outgoing_task = tokio::spawn(async move {
        while let Ok(message) = rx.recv().await {
            if let Err(e) = ws_sender.send(message).await {
                tracing::warn!("Failed to send WebSocket message: {:?}", e);
                break;
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = incoming_task => {
            tracing::info!("WebSocket incoming task completed");
        },
        _ = outgoing_task => {
            tracing::info!("WebSocket outgoing task completed");
        },
    }
    
    tracing::info!("WebSocket connection handler finished");
}
