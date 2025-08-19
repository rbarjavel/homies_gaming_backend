use crate::{
    errors::AppError,
    state::{MediaInfo, MediaType, MediaViewState, SoundInfo},
    templates::UploadTemplate,
    utils::{sanitize_filename, validate_file_path},
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
    tracing::info!("Serving upload form");
    let template = UploadTemplate;
    match template.render() {
        Ok(html) => {
            tracing::info!("Successfully rendered upload template");
            Ok(warp::reply::html(html))
        },
        Err(e) => {
            tracing::error!("Template render error: {}", e);
            Err(warp::reject::custom(AppError::RenderError(e)))
        },
    }
}

pub async fn upload_image(
    mut form: FormData,
    _addr: Option<std::net::SocketAddr>,
    state: SharedState,
    ws_clients: websocket::WsClients,
) -> Result<impl Reply, Rejection> {
    tracing::info!("Processing image upload");
    // Parse form data
    let form_data = parse_form_data(&mut form).await?;

    // Only proceed if we have a filename
    if !form_data.filename.is_empty() {
        tracing::info!("Processing file: {}", form_data.filename);
        // Validate file type
        if !is_valid_media_type(&form_data.filename) {
            tracing::warn!("Invalid file type uploaded: {}", form_data.filename);
            return Ok(warp::reply::html(
                "<p>Invalid file type! Only images and videos are allowed.</p>".to_string(),
            ));
        }

        // Save file to disk
        let file_size = save_uploaded_file(&form_data.filename, &form_data.file_data).await?;
        tracing::info!("Saved file to disk, size: {} bytes", file_size);

        // Check file size limit
        if file_size > 100 * 1024 * 1024 {
            // 100MB
            tracing::warn!("File too large: {} bytes", file_size);
            return Ok(warp::reply::html(
                "<p>File too large! Maximum size is 100MB.</p>".to_string(),
            ));
        }

        // Store values before move
        let mut filename = form_data.filename.clone();
        let caption = form_data.caption.clone();

        // Determine media type and adjust duration
        let media_type = detect_media_type(&form_data.filename);
        tracing::info!("Detected media type: {:?}", media_type);
        let final_duration = match media_type {
            MediaType::Video => 999999, // Special value for videos (no auto-refresh)
            MediaType::Image => form_data.duration_secs,
        };

        // Process video with caption overlay if it's a video and has a caption
        if media_type == MediaType::Video && !caption.is_empty() {
            tracing::info!("Processing video with caption overlay");
            filename = process_video_with_caption(&filename, &caption).await?;
        }

        // Create media info (use processed filename and empty caption for videos since it's now embedded)
        let final_caption = if media_type == MediaType::Video && !caption.is_empty() {
            String::new() // Caption is now embedded in video, don't show separately
        } else {
            caption.clone()
        };

        let media_info =
            create_media_info(filename.clone(), media_type, final_duration, final_caption);

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

        tracing::info!("Upload completed successfully: {}", filename);
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

    tracing::warn!("No media uploaded");
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
    tracing::info!("Parsing form data");
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
                        tracing::info!("Processing image field");
                        // Get filename
                        filename = field.filename().unwrap_or("unnamed").to_string();
                        tracing::info!("Filename: {}", filename);

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
                        tracing::info!("Processing duration field");
                        let duration_str = read_field_as_string(field).await?;
                        if let Ok(parsed_duration) = duration_str.trim().parse::<u64>() {
                            // Clamp duration between 1 and 60 seconds
                            duration_secs = parsed_duration.clamp(1, 60);
                            tracing::info!("Parsed duration: {} seconds", duration_secs);
                        } else {
                            tracing::warn!("Failed to parse duration, using default: {} seconds", duration_secs);
                        }
                    }
                    "caption" => {
                        tracing::info!("Processing caption field");
                        caption = read_field_as_string(field).await?;
                        caption = caption.trim().to_string();
                        tracing::info!("Parsed caption: {}", caption);
                    }
                    _ => {
                        tracing::debug!("Unknown field: {}", field.name());
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to read field: {}", e);
                return Err(warp::reject::custom(AppError::MultipartError));
            }
        }
    }

    tracing::info!("Form data parsing completed");
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
    // Sanitize the filename to prevent path traversal
    let sanitized_filename = sanitize_filename(filename)
        .ok_or_else(|| {
            tracing::error!("Invalid filename provided: {}", filename);
            warp::reject::custom(AppError::IoError(std::io::Error::other("Invalid filename")))
        })?;
    
    // Validate the file path to ensure it's within the uploads directory
    let file_path = validate_file_path("uploads", &sanitized_filename)
        .ok_or_else(|| {
            tracing::error!("Invalid file path: {}", filename);
            warp::reject::custom(AppError::IoError(std::io::Error::other("Invalid file path")))
        })?;
    
    // Validate file content matches extension
    if !is_valid_file_content(&sanitized_filename, file_data) {
        tracing::error!("File content does not match extension for: {}", sanitized_filename);
        return Err(warp::reject::custom(AppError::IoError(std::io::Error::other(
            "File content does not match file extension"
        ))));
    }
    
    tracing::info!("Saving uploaded file: {} ({} bytes)", sanitized_filename, file_data.len());

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

    tracing::info!("File saved successfully: {}", file_path);
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
    let media_type = media_info.media_type; // MediaType implements Copy, no need to clone

    tracing::info!("Updating state with new media: {} ({:?})", filename, media_type);

    // Update shared state
    let mut state = state.write().await;
    state.set_last_media(media_info);

    tracing::info!("New media uploaded: {}", filename);

    // Broadcast to websocket clients
    if media_type != MediaType::Video {
        tracing::info!("Broadcasting new media event");
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
    tracing::info!("Processing sound upload");
    let mut original_filename = String::new();
    let mut file_data = Vec::new();

    // Process the stream directly without collecting
    while let Some(result) = form.next().await {
        match result {
            Ok(mut field) => {
                match field.name() {
                    "sound" => {
                        tracing::info!("Processing sound field");
                        // Get filename
                        original_filename = field.filename().unwrap_or("unnamed").to_string();
                        tracing::info!("Sound filename: {}", original_filename);

                        // Validate sound file type
                        if !is_valid_sound_type(&original_filename) {
                            tracing::warn!("Invalid sound file type: {}", original_filename);
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
                    _ => {
                        tracing::debug!("Unknown field in sound upload: {}", field.name());
                    }
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
        tracing::info!("Processing sound file: {} ({} bytes)", original_filename, file_data.len());
        // Check file size limit (50MB for sounds)
        if file_data.len() > 50 * 1024 * 1024 {
            tracing::warn!("Sound file too large: {} bytes", file_data.len());
            return Ok(warp::reply::html(
                "<p>Sound file too large! Maximum size is 50MB.</p>".to_string(),
            ));
        }

        // Sanitize the filename to prevent path traversal
        let sanitized_filename = sanitize_filename(&original_filename)
            .ok_or_else(|| {
                tracing::error!("Invalid sound filename provided: {}", original_filename);
                warp::reject::custom(AppError::IoError(std::io::Error::other("Invalid filename")))
            })?;
        
        // Validate the file path to ensure it's within the sounds directory
        let file_path = validate_file_path("sounds", &sanitized_filename)
            .ok_or_else(|| {
                tracing::error!("Invalid sound file path: {}", original_filename);
                warp::reject::custom(AppError::IoError(std::io::Error::other("Invalid file path")))
            })?;
            
        // Validate file content matches extension
        if !is_valid_sound_content(&sanitized_filename, &file_data) {
            tracing::error!("Sound file content does not match extension for: {}", sanitized_filename);
            return Err(warp::reject::custom(AppError::IoError(std::io::Error::other(
                "Sound file content does not match file extension"
            ))));
        }

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
            filename: sanitized_filename.clone(),
            upload_time: std::time::SystemTime::now(),
            marked_for_deletion: false,
        };

        let mut state = state.write().await;
        state.set_last_sound(sound_info);
        tracing::info!("New sound uploaded: {}", sanitized_filename);
        websocket::broadcast_new_song(&ws_clients, sanitized_filename.clone()).await;

        return Ok(warp::reply::html(format!(
            r#"<p>Sound {} uploaded successfully!</p>"#,
            sanitized_filename
        )));
    }

    tracing::warn!("No sound file uploaded");
    Ok(warp::reply::html(
        "<p>No sound file uploaded!</p>".to_string(),
    ))
}

// Add sound type validation
fn is_valid_sound_type(filename: &str) -> bool {
    let ext = filename.split('.').next_back().unwrap_or("").to_lowercase();
    matches!(ext.as_str(), "mp3" | "wav" | "ogg" | "flac" | "m4a")
}

// Process video with caption overlay using ffmpeg
async fn process_video_with_caption(
    original_filename: &str,
    caption: &str,
) -> Result<String, Rejection> {
    tracing::info!("Processing video with caption overlay: {}", original_filename);
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
            tracing::info!(
                "Successfully processed video with caption: {}",
                output_filename
            );

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
    tracing::info!("Processing video URL upload");
    let video_url = form
        .get("video_url")
        .cloned()
        .or_else(|| form.get("youtube_url").cloned()) // Backward compatibility
        .unwrap_or_default();
    let caption = form.get("caption").cloned().unwrap_or_default();

    if video_url.is_empty() {
        tracing::warn!("No video URL provided");
        return Ok(warp::reply::html(
            "<p>No video URL provided!</p>".to_string(),
        ));
    }

    tracing::info!("Downloading video from URL: {}", video_url);

    // Check if yt-dlp is available
    if !VideoProcessor::is_ytdlp_available() {
        tracing::error!("Video download not available. yt-dlp is not installed.");
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
            return Ok(warp::reply::html(format!("<p>{}</p>", user_error)));
        }
    };

    tracing::info!("Video info - Title: {}, Duration: {}s, Uploader: {}", 
                   video_info.title, video_info.duration, video_info.uploader);

    // Check video duration (limit to reasonable length)
    if video_info.duration > 600 {
        // 10 minutes
        tracing::warn!("Video too long: {} seconds", video_info.duration);
        return Ok(warp::reply::html(
            "<p>Video too long! Maximum duration is 10 minutes.</p>".to_string(),
        ));
    }

    // Use streaming download and processing for better performance
    let filename = match VideoProcessor::stream_process_video(&video_url, "uploads", 
        if !caption.is_empty() { Some(&caption) } else { None }).await {
        Ok(filename) => {
            tracing::info!("Successfully downloaded and processed video: {}", filename);
            filename
        },
        Err(e) => {
            tracing::error!("Failed to download/process video: {}", e);
            let user_error = VideoProcessor::get_user_friendly_error(&e.to_string(), &video_url);
            return Ok(warp::reply::html(format!("<p>{}</p>", user_error)));
        }
    };

    // Create media info
    let media_info = create_media_info(
        filename.clone(),
        MediaType::Video,
        999999,        // Videos play full duration
        String::new(), // Caption is embedded if provided
    );

    // Update shared state and broadcast video event
    update_state_and_broadcast(state, media_info, ws_clients.clone()).await?;

    // Broadcast the video event for video downloads
    websocket::broadcast_video_event(&ws_clients, filename.clone()).await;

    // Return success response
    let caption_message = if !caption.is_empty() {
        "<br/>Caption embedded in video"
    } else {
        ""
    };

    tracing::info!("Video URL upload completed successfully");
    Ok(warp::reply::html(format!(
        r#"<p>Downloaded "{}" successfully!<br/>Duration: {} seconds{}</p>"#,
        video_info.title, video_info.duration, caption_message
    )))
}

/// Validate file content matches the file extension
fn is_valid_file_content(filename: &str, data: &[u8]) -> bool {
    let ext = filename.split('.').next_back().unwrap_or("").to_lowercase();
    
    match ext.as_str() {
        // Image formats
        "jpg" | "jpeg" => data.starts_with(&[0xFF, 0xD8, 0xFF]),
        "png" => data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
        "gif" => data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a"),
        "webp" => data.starts_with(b"RIFF") && data.len() > 12 && data[8..12] == *b"WEBP",
        "bmp" => data.starts_with(b"BM"),
        "svg" => data.starts_with(b"<?xml") || data.starts_with(b"<svg"),
        
        // Video formats
        "mp4" => data.starts_with(b"\x00\x00\x00\x18ftypmp42") || 
                 data.starts_with(b"\x00\x00\x00\x20ftypmp42") ||
                 data.starts_with(b"\x00\x00\x00\x18ftypmp41") ||
                 data.starts_with(b"\x00\x00\x00\x18ftypiso5"),
        "mov" | "m4v" => data.starts_with(b"\x00\x00\x00\x14ftypqt") || 
                         data.starts_with(b"\x00\x00\x00\x20ftypM4V"),
        "avi" => data.starts_with(b"RIFF") && data.len() > 8 && data[8..12] == *b"AVI ",
        "webm" => data.starts_with(b"\x1A\x45\xDF\xA3"),
        "mkv" => data.starts_with(b"\x1A\x45\xDF\xA3"),
        "ogg" => data.starts_with(b"OggS"),
        "wmv" => data.starts_with(b"\x30\x26\xB2\x75\x8E\x66\xCF\x11"),
        "flv" => data.starts_with(b"FLV\x01"),
        
        // If we don't recognize the extension, we'll allow it (better to be permissive than restrictive)
        _ => true,
    }
}

/// Validate sound file content matches the file extension
fn is_valid_sound_content(filename: &str, data: &[u8]) -> bool {
    let ext = filename.split('.').next_back().unwrap_or("").to_lowercase();
    
    match ext.as_str() {
        "mp3" => data.starts_with(&[0xFF, 0xFB]) || // MP3 with ID3v2
                 data.starts_with(&[0x49, 0x44, 0x33]) || // ID3v2 header
                 data.starts_with(&[0xFF, 0xF3]) || // MP3 without ID3
                 data.starts_with(&[0xFF, 0xF2]),
        "wav" => data.starts_with(b"RIFF") && data.len() > 8 && data[8..12] == *b"WAVE",
        "ogg" => data.starts_with(b"OggS"),
        "flac" => data.starts_with(b"fLaC"),
        "m4a" => data.starts_with(b"\x00\x00\x00\x20ftypM4A") ||
                 data.starts_with(b"\x00\x00\x00\x18ftypmp42") ||
                 data.starts_with(b"\x00\x00\x00\x18ftypM4A "),
        // If we don't recognize the extension, we'll allow it
        _ => true,
    }
}
