use crate::{
    errors::AppError,
    state::{MediaInfo, MediaType, MediaViewState},
    templates::UploadTemplate,
};
use askama::Template;
use bytes::Buf;
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::{fs::File, io::AsyncWriteExt};
use warp::{Rejection, Reply, multipart::FormData};

use crate::websocket;

// Shared state type
pub type SharedState = Arc<RwLock<MediaViewState>>;

pub async fn upload_form() -> Result<impl Reply, Rejection> {
    let template = UploadTemplate;
    match template.render() {
        Ok(html) => Ok(warp::reply::html(html)),
        Err(e) => Err(warp::reject::custom(AppError::RenderError(e))),
    }
}

pub async fn upload_image(
    mut form: FormData,
    _addr: Option<std::net::SocketAddr>,
    state: SharedState,
    ws_clients: websocket::WsClients, // Add this parameter
) -> Result<impl Reply, Rejection> {
    // Process the stream directly without collecting
    while let Some(result) = form.next().await {
        match result {
            Ok(mut field) => {
                if field.name() == "image" {
                    // Get filename early
                    let original_filename = field.filename().unwrap_or("unnamed").to_string();
                    let file_path = format!("uploads/{}", original_filename);

                    // Determine media type from filename
                    let media_type = detect_media_type(&original_filename);

                    // Create directory
                    tokio::fs::create_dir_all("uploads").await.map_err(|e| {
                        tracing::error!("Failed to create uploads directory: {}", e);
                        warp::reject::custom(AppError::IoError(e))
                    })?;

                    // Create file
                    let mut file = File::create(&file_path).await.map_err(|e| {
                        tracing::error!("Failed to create file: {}", e);
                        warp::reject::custom(AppError::IoError(e))
                    })?;

                    // Stream data chunks directly
                    while let Some(chunk_result) = field.data().await {
                        match chunk_result {
                            Ok(mut chunk) => {
                                // Convert Buf to bytes
                                let bytes = chunk.copy_to_bytes(chunk.remaining());
                                file.write_all(&bytes).await.map_err(|e| {
                                    tracing::error!("Failed to write file: {}", e);
                                    warp::reject::custom(AppError::IoError(e))
                                })?;
                            }
                            Err(e) => {
                                tracing::error!("Failed to read chunk: {}", e);
                                let io_err = std::io::Error::new(std::io::ErrorKind::Other, e);
                                return Err(warp::reject::custom(AppError::IoError(io_err)));
                            }
                        }
                    }

                    // Update shared state with new media
                    let media_info = MediaInfo {
                        filename: original_filename.clone(),
                        media_type,
                        upload_time: std::time::SystemTime::now(),
                        marked_for_deletion: false, // Add this field!
                    };

                    let mut state = state.write().await;
                    state.set_last_media(media_info);
                    tracing::info!("New media uploaded: {}", original_filename);
                    websocket::broadcast_new_media(&ws_clients).await;

                    return Ok(warp::reply::html(format!(
                        r#"<p>Uploaded {} successfully! <img src="/uploads/{}" style="max-width: 300px;" /></p>"#,
                        original_filename, original_filename
                    )));
                }
            }
            Err(e) => {
                tracing::error!("Failed to read field: {}", e);
                return Err(warp::reject::custom(AppError::MultipartError));
            }
        }
    }

    Ok(warp::reply::html("<p>No media uploaded!</p>".to_string()))
}

fn detect_media_type(filename: &str) -> MediaType {
    let ext = filename.split('.').last().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "mp4" | "mov" | "avi" | "webm" | "ogg" => MediaType::Video,
        _ => MediaType::Image, // Default to image for jpg, png, gif, etc.
    }
}
