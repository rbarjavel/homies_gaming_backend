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
    ws_clients: websocket::WsClients,
) -> Result<impl Reply, Rejection> {
    let mut original_filename = String::new();
    let mut duration_secs = 5u64; // Default duration
    let mut caption = String::new(); // Default caption

    // Process the stream directly without collecting
    while let Some(result) = form.next().await {
        match result {
            Ok(mut field) => {
                match field.name() {
                    "image" => {
                        // Get filename early
                        original_filename = field.filename().unwrap_or("unnamed").to_string();
                        let file_path = format!("uploads/{}", original_filename);

                        // Determine media type from filename
                        let media_type = detect_media_type(&original_filename);

                        // Validate file type
                        if !is_valid_media_type(&original_filename) {
                            return Ok(warp::reply::html(
                                "<p>Invalid file type! Only images and videos are allowed.</p>"
                                    .to_string(),
                            ));
                        }

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

                        // Stream data chunks directly and track file size
                        let mut total_size = 0u64;
                        const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024; // 100MB

                        while let Some(chunk_result) = field.data().await {
                            match chunk_result {
                                Ok(mut chunk) => {
                                    total_size += chunk.remaining() as u64;

                                    // Check file size limit
                                    if total_size > MAX_FILE_SIZE {
                                        tracing::error!("File too large: {} bytes", total_size);
                                        return Ok(warp::reply::html(
                                            "<p>File too large! Maximum size is 100MB.</p>"
                                                .to_string(),
                                        ));
                                    }

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
                    }
                    "duration" => {
                        // Parse duration from form field
                        let mut duration_bytes = Vec::new();
                        while let Some(chunk_result) = field.data().await {
                            match chunk_result {
                                Ok(mut chunk) => {
                                    let bytes = chunk.copy_to_bytes(chunk.remaining());
                                    duration_bytes.extend_from_slice(&bytes);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to read duration field: {}", e);
                                    return Err(warp::reject::custom(AppError::MultipartError));
                                }
                            }
                        }

                        if let Ok(duration_str) = String::from_utf8(duration_bytes) {
                            if let Ok(parsed_duration) = duration_str.trim().parse::<u64>() {
                                // Clamp duration between 1 and 60 seconds
                                duration_secs = parsed_duration.clamp(1, 60);
                            }
                        }
                    }
                    "caption" => {
                        // Parse caption from form field
                        let mut caption_bytes = Vec::new();
                        while let Some(chunk_result) = field.data().await {
                            match chunk_result {
                                Ok(mut chunk) => {
                                    let bytes = chunk.copy_to_bytes(chunk.remaining());
                                    caption_bytes.extend_from_slice(&bytes);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to read caption field: {}", e);
                                    return Err(warp::reject::custom(AppError::MultipartError));
                                }
                            }
                        }

                        if let Ok(parsed_caption) = String::from_utf8(caption_bytes) {
                            caption = parsed_caption.trim().to_string();
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => {
                tracing::error!("Failed to read field: {}", e);
                return Err(warp::reject::custom(AppError::MultipartError));
            }
        }
    }

    // Only proceed if we have a filename
    if !original_filename.is_empty() {
        // Determine media type and adjust duration
        let media_type = detect_media_type(&original_filename);
        // For videos, we might want to use a longer default or special handling
        let final_duration = match media_type {
            MediaType::Video => 999999, // Special value for videos (no auto-refresh)
            MediaType::Image => duration_secs,
        };

        // Update shared state with new media
        let media_info = MediaInfo {
            filename: original_filename.clone(),
            media_type,
            upload_time: std::time::SystemTime::now(),
            marked_for_deletion: false,
            duration_secs: final_duration,
            caption: caption.clone(), // Add caption
        };

        let mut state = state.write().await;
        state.set_last_media(media_info);
        tracing::info!(
            "New media uploaded: {} ({} bytes, {}s duration, caption: {})",
            original_filename,
            0,
            final_duration,
            if caption.is_empty() { "none" } else { &caption }
        );
        websocket::broadcast_new_media(&ws_clients).await;

        return Ok(warp::reply::html(format!(
            r#"<p>Uploaded {} successfully! Display duration: {} seconds{}</p>"#,
            original_filename,
            if final_duration == 999999 {
                "Full video".to_string()
            } else {
                final_duration.to_string()
            },
            if caption.is_empty() {
                String::new()
            } else {
                format!("<br/>Caption: {}", caption)
            }
        )));
    }

    Ok(warp::reply::html("<p>No media uploaded!</p>".to_string()))
}

fn detect_media_type(filename: &str) -> MediaType {
    let ext = filename.split('.').last().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "mp4" | "mov" | "avi" | "webm" | "ogg" | "mkv" | "wmv" | "flv" | "m4v" => MediaType::Video,
        _ => MediaType::Image, // Default to image for jpg, png, gif, etc.
    }
}

fn is_valid_media_type(filename: &str) -> bool {
    let ext = filename.split('.').last().unwrap_or("").to_lowercase();
    match ext.as_str() {
        // Images
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tiff" | "svg" => true,
        // Videos
        "mp4" | "mov" | "avi" | "webm" | "ogg" | "mkv" | "wmv" | "flv" | "m4v" => true,
        _ => false,
    }
}
