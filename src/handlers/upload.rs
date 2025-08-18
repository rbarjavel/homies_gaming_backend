use crate::{
    errors::AppError,
    state::{MediaInfo, MediaType, MediaViewState, SoundInfo},
    templates::UploadTemplate,
    video_processing::VideoProcessor,
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
    // Parse form data
    let form_data = parse_form_data(&mut form).await?;

    // Only proceed if we have a filename
    if !form_data.filename.is_empty() {
        // Validate file type
        if !is_valid_media_type(&form_data.filename) {
            return Ok(warp::reply::html(
                "<p>Invalid file type! Only images and videos are allowed.</p>".to_string(),
            ));
        }

        // Save file to disk
        let file_size = save_uploaded_file(&form_data.filename, &form_data.file_data).await?;

        // Check file size limit
        if file_size > 100 * 1024 * 1024 {
            // 100MB
            return Ok(warp::reply::html(
                "<p>File too large! Maximum size is 100MB.</p>".to_string(),
            ));
        }

        // Store values before move
        let mut filename = form_data.filename.clone();
        let caption = form_data.caption.clone();

        // Determine media type and adjust duration
        let media_type = detect_media_type(&form_data.filename);
        let final_duration = match media_type {
            MediaType::Video => 999999, // Special value for videos (no auto-refresh)
            MediaType::Image => form_data.duration_secs,
        };

        // Process video with caption overlay if it's a video and has a caption
        if media_type == MediaType::Video && !caption.is_empty() {
            filename = process_video_with_caption(&filename, &caption).await?;
        }

        // Create media info (use processed filename and empty caption for videos since it's now embedded)
        let final_caption = if media_type == MediaType::Video && !caption.is_empty() {
            String::new() // Caption is now embedded in video, don't show separately
        } else {
            caption.clone()
        };
        
        let media_info = create_media_info(
            filename.clone(),
            media_type,
            final_duration,
            final_caption,
        );

        // Update shared state and broadcast appropriate events
        update_state_and_broadcast(state, media_info.clone(), ws_clients.clone()).await?;
        
        // If it's a video, also broadcast the video event
        if media_type == MediaType::Video {
            websocket::broadcast_video_event(&ws_clients, filename.clone()).await;
        }

        // Return success response
        let caption_message = if media_type == MediaType::Video && !caption.is_empty() {
            "<br/>Caption embedded in video"
        } else if !caption.is_empty() {
            &format!("<br/>Caption: {}", caption)
        } else {
            ""
        };
        
        return Ok(warp::reply::html(format!(
            r#"<p>Uploaded {} successfully! Display duration: {} seconds{}</p>"#,
            filename,
            if final_duration == 999999 {
                "Full video".to_string()
            } else {
                final_duration.to_string()
            },
            caption_message
        )));
    }

    Ok(warp::reply::html("<p>No media uploaded!</p>".to_string()))
}

// Struct to hold parsed form data
struct FormDataParsed {
    filename: String,
    file_data: Vec<u8>,
    duration_secs: u64,
    caption: String,
}

// Parse form data from multipart
async fn parse_form_data(form: &mut FormData) -> Result<FormDataParsed, Rejection> {
    let mut filename = String::new();
    let mut file_data = Vec::new();
    let mut duration_secs = 5u64; // Default duration
    let mut caption = String::new(); // Default caption

    // Process the stream directly without collecting
    while let Some(result) = form.next().await {
        match result {
            Ok(mut field) => {
                match field.name() {
                    "image" => {
                        // Get filename
                        filename = field.filename().unwrap_or("unnamed").to_string();

                        // Collect file data
                        while let Some(chunk_result) = field.data().await {
                            match chunk_result {
                                Ok(mut chunk) => {
                                    let bytes = chunk.copy_to_bytes(chunk.remaining());
                                    file_data.extend_from_slice(&bytes);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to read file data: {}", e);
                                    return Err(warp::reject::custom(AppError::MultipartError));
                                }
                            }
                        }
                    }
                    "duration" => {
                        let duration_str = read_field_as_string(field).await?;
                        if let Ok(parsed_duration) = duration_str.trim().parse::<u64>() {
                            // Clamp duration between 1 and 60 seconds
                            duration_secs = parsed_duration.clamp(1, 60);
                        }
                    }
                    "caption" => {
                        caption = read_field_as_string(field).await?;
                        caption = caption.trim().to_string();
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

    Ok(FormDataParsed {
        filename,
        file_data,
        duration_secs,
        caption,
    })
}

// Read a form field as a string
async fn read_field_as_string(mut field: warp::multipart::Part) -> Result<String, Rejection> {
    let mut field_data = Vec::new();
    while let Some(chunk_result) = field.data().await {
        match chunk_result {
            Ok(mut chunk) => {
                let bytes = chunk.copy_to_bytes(chunk.remaining());
                field_data.extend_from_slice(&bytes);
            }
            Err(e) => {
                tracing::error!("Failed to read field data: {}", e);
                return Err(warp::reject::custom(AppError::MultipartError));
            }
        }
    }

    String::from_utf8(field_data).map_err(|e| {
        tracing::error!("Failed to parse field as UTF-8: {}", e);
        warp::reject::custom(AppError::MultipartError)
    })
}

// Save uploaded file to disk
async fn save_uploaded_file(filename: &str, file_data: &[u8]) -> Result<u64, Rejection> {
    let file_path = format!("uploads/{}", filename);

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

    // Write file data
    file.write_all(file_data).await.map_err(|e| {
        tracing::error!("Failed to write file: {}", e);
        warp::reject::custom(AppError::IoError(e))
    })?;

    Ok(file_data.len() as u64)
}

// Create MediaInfo struct
fn create_media_info(
    filename: String,
    media_type: MediaType,
    duration_secs: u64,
    caption: String,
) -> MediaInfo {
    MediaInfo {
        filename,
        media_type,
        upload_time: std::time::SystemTime::now(),
        marked_for_deletion: false,
        duration_secs,
        caption,
    }
}

// Update state and broadcast new media
async fn update_state_and_broadcast(
    state: SharedState,
    media_info: MediaInfo,
    ws_clients: websocket::WsClients,
) -> Result<(), Rejection> {
    let filename = media_info.filename.clone();
    let media_type = media_info.media_type.clone();

    // Update shared state
    let mut state = state.write().await;
    state.set_last_media(media_info);

    tracing::info!("New media uploaded: {}", filename);

    // Broadcast to websocket clients
    if media_type != MediaType::Video {
        websocket::broadcast_new_media(&ws_clients).await;
    }

    Ok(())
}

fn detect_media_type(filename: &str) -> MediaType {
    let ext = filename.split('.').next_back().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "mp4" | "mov" | "avi" | "webm" | "ogg" | "mkv" | "wmv" | "flv" | "m4v" => MediaType::Video,
        _ => MediaType::Image, // Default to image for jpg, png, gif, etc.
    }
}

fn is_valid_media_type(filename: &str) -> bool {
    let ext = filename.split('.').next_back().unwrap_or("").to_lowercase();
    match ext.as_str() {
        // Images
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tiff" | "svg" => true,
        // Videos
        "mp4" | "mov" | "avi" | "webm" | "ogg" | "mkv" | "wmv" | "flv" | "m4v" => true,
        _ => false,
    }
}

pub async fn upload_sound(
    mut form: FormData,
    _addr: Option<std::net::SocketAddr>,
    state: SharedState,
    ws_clients: websocket::WsClients,
) -> Result<impl Reply, Rejection> {
    let mut original_filename = String::new();
    let mut file_data = Vec::new();

    // Process the stream directly without collecting
    while let Some(result) = form.next().await {
        match result {
            Ok(mut field) => {
                match field.name() {
                    "sound" => {
                        // Get filename
                        original_filename = field.filename().unwrap_or("unnamed").to_string();

                        // Validate sound file type
                        if !is_valid_sound_type(&original_filename) {
                            return Ok(warp::reply::html(
                                "<p>Invalid sound file type! Only MP3, WAV, and OGG files are allowed.</p>".to_string(),
                            ));
                        }

                        // Collect file data
                        while let Some(chunk_result) = field.data().await {
                            match chunk_result {
                                Ok(mut chunk) => {
                                    let bytes = chunk.copy_to_bytes(chunk.remaining());
                                    file_data.extend_from_slice(&bytes);
                                }
                                Err(e) => {
                                    tracing::error!("Failed to read sound file data: {}", e);
                                    return Err(warp::reject::custom(AppError::MultipartError));
                                }
                            }
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
        // Check file size limit (50MB for sounds)
        if file_data.len() > 50 * 1024 * 1024 {
            return Ok(warp::reply::html(
                "<p>Sound file too large! Maximum size is 50MB.</p>".to_string(),
            ));
        }

        // Save sound file to disk
        let file_path = format!("sounds/{}", original_filename);

        // Create directory
        tokio::fs::create_dir_all("sounds").await.map_err(|e| {
            tracing::error!("Failed to create sounds directory: {}", e);
            warp::reject::custom(AppError::IoError(e))
        })?;

        // Create file
        let mut file = File::create(&file_path).await.map_err(|e| {
            tracing::error!("Failed to create sound file: {}", e);
            warp::reject::custom(AppError::IoError(e))
        })?;

        // Write file data
        file.write_all(&file_data).await.map_err(|e| {
            tracing::error!("Failed to write sound file: {}", e);
            warp::reject::custom(AppError::IoError(e))
        })?;

        // Update shared state with new sound
        let sound_info = SoundInfo {
            filename: original_filename.clone(),
            upload_time: std::time::SystemTime::now(),
            marked_for_deletion: false,
        };

        let mut state = state.write().await;
        state.set_last_sound(sound_info);
        tracing::info!("New sound uploaded: {}", original_filename);
        websocket::broadcast_new_song(&ws_clients, original_filename.clone()).await;

        return Ok(warp::reply::html(format!(
            r#"<p>Sound {} uploaded successfully!</p>"#,
            original_filename
        )));
    }

    Ok(warp::reply::html(
        "<p>No sound file uploaded!</p>".to_string(),
    ))
}

// Add sound type validation
fn is_valid_sound_type(filename: &str) -> bool {
    let ext = filename.split('.').next_back().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "mp3" | "wav" | "ogg" | "flac" | "m4a" => true,
        _ => false,
    }
}

// Process video with caption overlay using ffmpeg
async fn process_video_with_caption(
    original_filename: &str,
    caption: &str,
) -> Result<String, Rejection> {
    // Check if ffmpeg is available
    if !VideoProcessor::is_ffmpeg_available() {
        tracing::warn!("FFmpeg not available, skipping caption overlay");
        return Ok(original_filename.to_string());
    }

    // Generate output filename
    let output_filename = VideoProcessor::generate_output_filename(original_filename);
    
    let input_path = format!("uploads/{}", original_filename);
    let output_path = format!("uploads/{}", output_filename);

    // Process video with caption overlay
    match VideoProcessor::add_caption_overlay(&input_path, &output_path, caption).await {
        Ok(_) => {
            tracing::info!("Successfully processed video with caption: {}", output_filename);
            
            // Remove original file to save space
            if let Err(e) = tokio::fs::remove_file(&input_path).await {
                tracing::warn!("Failed to remove original video file {}: {}", input_path, e);
            }
            
            Ok(output_filename)
        }
        Err(e) => {
            tracing::error!("Failed to process video with caption: {}", e);
            // Return original filename if processing fails
            Ok(original_filename.to_string())
        }
    }
}

// Video upload handler (YouTube, TikTok)
pub async fn upload_video_url(
    form: std::collections::HashMap<String, String>,
    state: SharedState,
    ws_clients: websocket::WsClients,
) -> Result<impl Reply, Rejection> {
    let video_url = form.get("video_url").cloned()
        .or_else(|| form.get("youtube_url").cloned()) // Backward compatibility
        .unwrap_or_default();
    let caption = form.get("caption").cloned().unwrap_or_default();

    if video_url.is_empty() {
        return Ok(warp::reply::html(
            "<p>No video URL provided!</p>".to_string(),
        ));
    }

    // Check if yt-dlp is available
    if !VideoProcessor::is_ytdlp_available() {
        return Ok(warp::reply::html(
            "<p>Video download not available. yt-dlp is not installed.</p>".to_string(),
        ));
    }

    // Get video info first
    let video_info = match VideoProcessor::get_video_metadata(&video_url).await {
        Ok(info) => info,
        Err(e) => {
            tracing::error!("Failed to get video info: {}", e);
            let user_error = VideoProcessor::get_user_friendly_error(&e.to_string(), &video_url);
            return Ok(warp::reply::html(
                format!("<p>{}</p>", user_error),
            ));
        }
    };

    // Check video duration (limit to reasonable length)
    if video_info.duration > 600 { // 10 minutes
        return Ok(warp::reply::html(
            "<p>Video too long! Maximum duration is 10 minutes.</p>".to_string(),
        ));
    }

    // Download the video
    let filename = match VideoProcessor::download_video(&video_url, "uploads").await {
        Ok(filename) => filename,
        Err(e) => {
            tracing::error!("Failed to download video: {}", e);
            let user_error = VideoProcessor::get_user_friendly_error(&e.to_string(), &video_url);
            return Ok(warp::reply::html(
                format!("<p>{}</p>", user_error),
            ));
        }
    };

    // Process video with caption if provided
    let final_filename = if !caption.is_empty() {
        match process_video_with_caption(&filename, &caption).await {
            Ok(processed_filename) => processed_filename,
            Err(e) => {
                tracing::error!("Failed to process video with caption: {:?}", e);
                filename // Use original if caption processing fails
            }
        }
    } else {
        filename
    };

    // Create media info
    let media_info = create_media_info(
        final_filename.clone(),
        MediaType::Video,
        999999, // Videos play full duration
        String::new(), // Caption is embedded if provided
    );

    // Update shared state and broadcast video event
    update_state_and_broadcast(state, media_info, ws_clients.clone()).await?;
    
    // Broadcast the video event for video downloads
    websocket::broadcast_video_event(&ws_clients, final_filename.clone()).await;

    // Return success response
    let caption_message = if !caption.is_empty() {
        "<br/>Caption embedded in video"
    } else {
        ""
    };

    Ok(warp::reply::html(format!(
        r#"<p>Downloaded "{}" successfully!<br/>Duration: {} seconds{}</p>"#,
        video_info.title,
        video_info.duration,
        caption_message
    )))
}
