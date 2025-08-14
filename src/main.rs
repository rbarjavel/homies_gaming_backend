mod errors;
mod handlers;
mod state;
mod templates;
mod websocket; // Add this

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing_subscriber;
use warp::Filter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Create shared state
    let media_state = Arc::new(RwLock::new(state::MediaViewState::new()));

    // Create WebSocket state
    let ws_clients = websocket::create_ws_state();

    // Start background cleanup task
    start_cleanup_task(media_state.clone());

    // Clone for different routes
    let media_state_upload = media_state.clone();
    let media_state_media = media_state.clone();
    let ws_clients_upload = ws_clients.clone();
    let ws_clients_route = ws_clients.clone();

    // Index route
    let index_route = warp::get()
        .and(warp::path::end())
        .and_then(handlers::media::index_page);

    // Upload routes
    let upload_form_route = warp::get()
        .and(warp::path("upload"))
        .and_then(handlers::upload::upload_form);

    let upload_route = warp::post()
        .and(warp::path("upload"))
        .and(warp::multipart::form().max_length(100 * 1024 * 1024)) // 100MB limit
        .and(warp::addr::remote())
        .and(with_state(media_state_upload.clone()))
        .and(with_ws_state(ws_clients_upload.clone())) // Add WebSocket state
        .and_then(handlers::upload::upload_image);

    let upload_sound_route = warp::post()
        .and(warp::path("upload-sound"))
        .and(warp::multipart::form().max_length(50 * 1024 * 1024)) // 50MB limit for sounds
        .and(warp::addr::remote())
        .and(with_state(media_state_upload.clone()))
        .and(with_ws_state(ws_clients_upload.clone())) // Add WebSocket state
        .and_then(handlers::upload::upload_sound);

    // Media routes
    let last_media_route = warp::get()
        .and(warp::path("last-media"))
        .and(warp::addr::remote())
        .and(with_state(media_state_media))
        .and_then(handlers::media::last_media);

    // WebSocket route - THIS IS THE NEW PART
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(with_ws_state(ws_clients_route))
        .and_then(
            |ws: warp::ws::Ws, clients| async move { websocket::ws_handler(ws, clients).await },
        );

    // Serve uploaded files
    let uploads_dir = warp::path("uploads").and(warp::fs::dir("uploads/"));
    let sounds_dir = warp::path("sounds").and(warp::fs::dir("sounds/"));

    // Combine all routes
    let routes = index_route
        .or(upload_form_route)
        .or(upload_sound_route)
        .or(upload_route)
        .or(last_media_route)
        .or(ws_route) // Add WebSocket route
        .or(uploads_dir)
        .or(sounds_dir);

    println!("Server running on http://0.0.0.0:3030");
    warp::serve(routes).run(([0, 0, 0, 0], 3030)).await;
}

fn with_state(
    state: Arc<RwLock<state::MediaViewState>>,
) -> impl Filter<Extract = (Arc<RwLock<state::MediaViewState>>,), Error = std::convert::Infallible> + Clone
{
    warp::any().map(move || state.clone())
}

// Add WebSocket state filter
fn with_ws_state(
    clients: websocket::WsClients,
) -> impl Filter<Extract = (websocket::WsClients,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || clients.clone())
}

// Background cleanup task
fn start_cleanup_task(state: Arc<RwLock<state::MediaViewState>>) {
    tokio::spawn(async move {
        let deletion_threshold = Duration::from_secs(10);

        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let files_to_delete = {
                let state_guard = state.read().await;
                state_guard.get_files_to_delete(deletion_threshold)
            };

            for filename in files_to_delete {
                let file_path = format!("uploads/{}", filename);

                match tokio::fs::remove_file(&file_path).await {
                    Ok(_) => {
                        tracing::info!("Deleted file: {}", filename);

                        let mut state_guard = state.write().await;
                        state_guard.remove_file_from_state(&filename);
                    }
                    Err(e) => {
                        tracing::error!("Failed to delete file {}: {}", file_path, e);
                        let mut state_guard = state.write().await;
                        state_guard.mark_for_deletion(&filename);
                    }
                }
            }
        }
    });
}
