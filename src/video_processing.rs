use crate::errors::AppError;
use crate::utils::sanitize_filename;
use crate::websocket::WsClients;
use serde_json::Value;
use std::process::Command;
use tokio::process::Command as AsyncCommand;

pub struct VideoProcessor;

impl VideoProcessor {
    /// Check if yt-dlp is available on the system
    pub fn is_ytdlp_available() -> bool {
        Command::new("yt-dlp")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Stream video download directly to processing (most efficient approach)
    pub async fn stream_convert_video(url: &str, output_dir: &str, _caption: Option<&str>, ws_clients: Option<WsClients>) -> Result<String, AppError> {
        // Validate video URL
        if !Self::is_supported_video_url(url) {
            return Err(AppError::IoError(std::io::Error::other(
                "Invalid video URL. Supported platforms: YouTube, TikTok",
            )));
        }

        // Check if required tools are available
        if !Self::is_ytdlp_available() {
            return Err(AppError::IoError(std::io::Error::other(
                "yt-dlp is not available on the system",
            )));
        }

        // Validate output directory
        if output_dir != "uploads" {
            return Err(AppError::IoError(std::io::Error::other(
                "Invalid output directory",
            )));
        }

        // Create output directory
        tokio::fs::create_dir_all(output_dir).await.map_err(|e| {
            tracing::error!("Failed to create output directory: {}", e);
            AppError::IoError(std::io::Error::other("Failed to create output directory"))
        })?;

        // Create debug directory
        let debug_dir = "debug";
        tokio::fs::create_dir_all(debug_dir).await.map_err(|e| {
            tracing::error!("Failed to create debug directory: {}", e);
            AppError::IoError(std::io::Error::other("Failed to create debug directory"))
        })?;

        // Generate unique filename
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let output_filename = format!("video_{}.mp4", timestamp);
        // Sanitize the filename
        let sanitized_output_filename = sanitize_filename(&output_filename)
            .ok_or_else(|| AppError::IoError(std::io::Error::other("Invalid filename")))?;
        let output_path = format!("{}/{}", output_dir, sanitized_output_filename);

        // Send progress update: starting download
        if let Some(ref clients) = ws_clients {
            crate::websocket::broadcast_upload_status(clients, "Starting video download...".to_string(), false).await;
        }

        // Download the video using yt-dlp directly as MP4
        tracing::info!("Downloading video: {}", url);

        let mut download_cmd = AsyncCommand::new("yt-dlp");
        download_cmd.args([
            "--cookies",  // Use --cookies instead of --cookies-from-browser
            "./firefox_cookies.txt", // Path to your cookies file
            "--format",
            "bv*[ext=mp4][height<=720]+ba[ext=m4a]/bv*[ext=mp4][height<=720]/b[ext=mp4]",
            "--output",
            &output_path,
            "--no-playlist",
            url,
        ]);

        let download_output = download_cmd.output().await.map_err(|e| {
            // Send error progress update
            if let Some(ref clients) = ws_clients {
                let clients_clone = clients.clone();
                let error_msg = format!("Video download failed: {}", e);
                tokio::spawn(async move {
                    crate::websocket::broadcast_upload_status(&clients_clone, error_msg, true).await;
                });
            }
            tracing::error!("Failed to execute yt-dlp: {}", e);
            AppError::IoError(std::io::Error::other("Video download failed"))
        })?;

        if !download_output.status.success() {
            let stderr = String::from_utf8_lossy(&download_output.stderr);
            tracing::error!("yt-dlp download failed: {}", stderr);

            // Send error progress update
            if let Some(ref clients) = ws_clients {
                let clients_clone = clients.clone();
                tokio::spawn(async move {
                    crate::websocket::broadcast_upload_status(&clients_clone, "Video download failed".to_string(), true).await;
                });
            }

            // Clean up any partial file
            let _ = tokio::fs::remove_file(&output_path).await;

            // Check for specific TikTok authentication issues
            if stderr.contains("Log in for access") || stderr.contains("cookies") {
                return Err(AppError::IoError(std::io::Error::other(
                    "TikTok video requires authentication. This video may be age-restricted or private. Try a different public TikTok video.",
                )));
            }

            // Check for other TikTok-specific issues
            if stderr.contains("not comfortable for some audiences") {
                return Err(AppError::IoError(std::io::Error::other(
                    "TikTok video is age-restricted and cannot be downloaded without authentication. Please try a different video.",
                )));
            }

            // Check for private/unavailable content
            if stderr.contains("Private video") || stderr.contains("Video unavailable") {
                return Err(AppError::IoError(std::io::Error::other(
                    "Video is private or unavailable. Please check the URL and try again.",
                )));
            }
            
            // Check for app restriction errors (newer YouTube API issue)
            if stderr.contains("not available on this app") {
                return Err(AppError::IoError(std::io::Error::other(
                    "This video is not available for download due to YouTube's app restrictions. \
                     This might be resolved by updating yt-dlp. \
                     If you're running this application, please ensure yt-dlp is up to date.",
                )));
            }

            return Err(AppError::IoError(std::io::Error::other("Video download failed")));
        }

        // Send progress update: download completed
        if let Some(ref clients) = ws_clients {
            let clients_clone = clients.clone();
            tokio::spawn(async move {
                crate::websocket::broadcast_upload_status(&clients_clone, "Video download completed, starting re-encoding...".to_string(), false).await;
            });
        }

        // Check if the file was created
        if tokio::fs::metadata(&output_path).await.is_err() {
            // Send error progress update
            if let Some(ref clients) = ws_clients {
                let clients_clone = clients.clone();
                tokio::spawn(async move {
                    crate::websocket::broadcast_upload_status(&clients_clone, "Downloaded video file not found".to_string(), true).await;
                });
            }
            return Err(AppError::IoError(std::io::Error::other(
                "Downloaded video file not found",
            )));
        }

        // Debug: Copy downloaded video
        let debug_downloaded_path = format!("{}/downloaded_{}", debug_dir, sanitized_output_filename);
        let _ = tokio::fs::copy(&output_path, &debug_downloaded_path).await;

        // Re-encode to H.264 for Godot compatibility
        tracing::info!("Re-encoding video to H.264: {}", output_path);
        let reencoded_path = format!("{}_h264.mp4", output_path.trim_end_matches(".mp4"));
        let mut reencode_cmd = AsyncCommand::new("ffmpeg");
        reencode_cmd.args([
            "-i", &output_path,
            "-c:v", "libx264",
            "-preset", "ultrafast",  // fastest encoding
            "-crf", "23",            // default quality
            "-c:a", "aac",           // ensure aac audio
            "-vf", "scale='min(1280,iw)':-1",  // Scale to max 1280px width, maintain aspect ratio
            "-movflags", "+faststart",  // Optimize for web streaming
            "-y",                    // overwrite output
            &reencoded_path,
        ]);

        let reencode_output = reencode_cmd.output().await.map_err(|e| {
            // Send error progress update
            if let Some(ref clients) = ws_clients {
                let clients_clone = clients.clone();
                let error_msg = format!("Video re-encoding failed: {}", e);
                tokio::spawn(async move {
                    crate::websocket::broadcast_upload_status(&clients_clone, error_msg, true).await;
                });
            }
            tracing::error!("Failed to execute ffmpeg: {}", e);
            AppError::IoError(std::io::Error::other("Video re-encoding failed"))
        })?;

        if !reencode_output.status.success() {
            let stderr = String::from_utf8_lossy(&reencode_output.stderr);
            tracing::error!("ffmpeg re-encoding failed: {}", stderr);

            // Send error progress update
            if let Some(ref clients) = ws_clients {
                let clients_clone = clients.clone();
                tokio::spawn(async move {
                    crate::websocket::broadcast_upload_status(&clients_clone, "Video re-encoding failed".to_string(), true).await;
                });
            }

            // Clean up files
            let _ = tokio::fs::remove_file(&output_path).await;
            let _ = tokio::fs::remove_file(&reencoded_path).await;

            return Err(AppError::IoError(std::io::Error::other("Video re-encoding failed")));
        }

        // Send progress update: re-encoding completed
        if let Some(ref clients) = ws_clients {
            let clients_clone = clients.clone();
            tokio::spawn(async move {
                crate::websocket::broadcast_upload_status(&clients_clone, "Video re-encoding completed, finalizing...".to_string(), false).await;
            });
        }

        // Debug: Copy re-encoded video before replacing original
        let debug_reencoded_path = format!("{}/reencoded_{}", debug_dir, sanitized_output_filename);
        let _ = tokio::fs::copy(&reencoded_path, &debug_reencoded_path).await;

        // Remove original file and rename re-encoded file
        let _ = tokio::fs::remove_file(&output_path).await;
        tokio::fs::rename(&reencoded_path, &output_path).await.map_err(|e| {
            // Send error progress update
            if let Some(ref clients) = ws_clients {
                let clients_clone = clients.clone();
                let error_msg = format!("Failed to finalize video processing: {}", e);
                tokio::spawn(async move {
                    crate::websocket::broadcast_upload_status(&clients_clone, error_msg, true).await;
                });
            }
            tracing::error!("Failed to rename re-encoded video: {}", e);
            AppError::IoError(std::io::Error::other("Failed to process video"))
        })?;

        // Debug: Copy final video
        let debug_final_path = format!("{}/final_{}", debug_dir, sanitized_output_filename);
        let _ = tokio::fs::copy(&output_path, &debug_final_path).await;

        tracing::info!("Video downloaded and re-encoded successfully: {}", output_path);

        // Send progress update: process completed
        if let Some(ref clients) = ws_clients {
            let clients_clone = clients.clone();
            tokio::spawn(async move {
                crate::websocket::broadcast_upload_status(&clients_clone, "Video processing completed successfully!".to_string(), false).await;
            });
        }

        // Return the filename
        Ok(sanitized_output_filename)
    }

    /// Get video metadata from supported platforms (YouTube, TikTok)
    pub async fn get_video_metadata(url: &str) -> Result<VideoMetadata, AppError> {
        if !Self::is_supported_video_url(url) {
            return Err(AppError::IoError(std::io::Error::other(
                "Invalid video URL. Supported platforms: YouTube, TikTok",
            )));
        }

        if !Self::is_ytdlp_available() {
            return Err(AppError::IoError(std::io::Error::other(
                "yt-dlp is not available on the system",
            )));
        }

        let mut cmd = AsyncCommand::new("yt-dlp");
        cmd.args([
            "--cookies",  // Use --cookies instead of --cookies-from-browser
            "./firefox_cookies.txt", // Path to your cookies file
            "--dump-json",
            "--no-playlist",
            url,
        ]);

        let output = cmd.output().await.map_err(|e| {
            tracing::error!("Failed to execute yt-dlp for info: {}", e);
            AppError::IoError(std::io::Error::other("Failed to get video information"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("yt-dlp info failed: {}", stderr);

            // Check for specific TikTok authentication issues
            if stderr.contains("Log in for access") || stderr.contains("cookies") {
                return Err(AppError::IoError(std::io::Error::other(
                    "TikTok video requires authentication. This video may be age-restricted or private. Try a different public TikTok video.",
                )));
            }

            // Check for other TikTok-specific issues
            if stderr.contains("not comfortable for some audiences") {
                return Err(AppError::IoError(std::io::Error::other(
                    "TikTok video is age-restricted and cannot be downloaded without authentication. Please try a different video.",
                )));
            }

            // Check for private/unavailable content
            if stderr.contains("Private video") || stderr.contains("Video unavailable") {
                return Err(AppError::IoError(std::io::Error::other(
                    "Video is private or unavailable. Please check the URL and try again.",
                )));
            }
            
            // Check for app restriction errors (newer YouTube API issue)
            if stderr.contains("not available on this app") {
                return Err(AppError::IoError(std::io::Error::other(
                    "This video is not available for download due to YouTube's app restrictions. \
                     This might be resolved by updating yt-dlp. \
                     If you're running this application, please ensure yt-dlp is up to date.",
                )));
            }

            return Err(AppError::IoError(std::io::Error::other("Failed to get video information")));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let json: Value = serde_json::from_str(&json_str).map_err(|e| {
            tracing::error!("Failed to parse yt-dlp JSON output: {}", e);
            AppError::IoError(std::io::Error::other("Failed to parse video information"))
        })?;

        Ok(VideoMetadata {
            title: json["title"].as_str().unwrap_or("Unknown").to_string(),
            duration: json["duration"].as_u64().unwrap_or(0),
            uploader: json["uploader"].as_str().unwrap_or("Unknown").to_string(),
            platform: Self::detect_platform(url),
        })
    }

    pub async fn get_video_dimensions(video_path: &str) -> Result<(u16, u16), AppError> {
        let mut cmd = AsyncCommand::new("ffprobe");
        cmd.args([
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=width,height",
            "-of", "csv=p=0",
            video_path,
        ]);

        let output = cmd.output().await.map_err(|e| {
            tracing::error!("Failed to execute ffprobe: {}", e);
            AppError::IoError(std::io::Error::other("Failed to get video dimensions"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("ffprobe failed: {}", stderr);
            return Err(AppError::IoError(std::io::Error::other("Failed to get video dimensions")));
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = output_str.trim().split(',').collect();

        if parts.len() != 2 {
            return Err(AppError::IoError(std::io::Error::other("Invalid video dimensions output")));
        }

        let width = parts[0].parse::<u16>().map_err(|_| {
            AppError::IoError(std::io::Error::other("Failed to parse video width"))
        })?;

        let height = parts[1].parse::<u16>().map_err(|_| {
            AppError::IoError(std::io::Error::other("Failed to parse video height"))
        })?;

        Ok((width, height))
    }

    /// Check if URL is a valid video platform URL (YouTube or TikTok)
    fn is_supported_video_url(url: &str) -> bool {
        // YouTube URLs
        url.contains("youtube.com/watch") || 
        url.contains("youtu.be/") || 
        url.contains("youtube.com/shorts/") ||
        url.contains("m.youtube.com/watch") ||
        // TikTok URLs
        url.contains("tiktok.com/@") ||
        url.contains("vm.tiktok.com/") ||
        url.contains("vt.tiktok.com/") ||
        url.contains("tiktok.com/t/") ||
        url.contains("m.tiktok.com/")
    }

    /// Detect the platform from URL
    fn detect_platform(url: &str) -> VideoPlatform {
        if url.contains("youtube.com") || url.contains("youtu.be") {
            VideoPlatform::YouTube
        } else if url.contains("tiktok.com") {
            VideoPlatform::TikTok
        } else {
            VideoPlatform::YouTube // Default fallback
        }
    }

    /// Get user-friendly error message for common video download issues
    pub fn get_user_friendly_error(error_msg: &str, url: &str) -> String {
        let platform = Self::detect_platform(url);

        if error_msg.contains("Log in for access") || error_msg.contains("cookies") {
            match platform {
                VideoPlatform::TikTok => {
                    "This TikTok video requires login to view (age-restricted or sensitive content). Please try a different public TikTok video.".to_string()
                }
                VideoPlatform::YouTube => {
                    "This YouTube video requires authentication. Please try a different public video.".to_string()
                }
            }
        } else if error_msg.contains("not comfortable for some audiences") {
            "This video is age-restricted and cannot be downloaded. Please try a different video."
                .to_string()
        } else if error_msg.contains("Private video") || error_msg.contains("Video unavailable") {
            "This video is private or unavailable. Please check the URL and try again.".to_string()
        } else if error_msg.contains("Video too long") {
            "Video is too long (maximum 10 minutes allowed).".to_string()
        } else if error_msg.contains("not available on this app") {
            "This video is not available for download due to YouTube's app restrictions. \
             This might be resolved by updating yt-dlp. \
             If you're running this application, please ensure yt-dlp is up to date."
                .to_string()
        } else {
            match platform {
                VideoPlatform::TikTok => {
                    "Failed to download TikTok video. Make sure it's a public, non-restricted video and try again.".to_string()
                }
                VideoPlatform::YouTube => {
                    "Failed to download YouTube video. Please check the URL and try again.".to_string()
                }
            }
        }
    }
}

/// Video platform types
#[derive(Debug, Clone, PartialEq)]
pub enum VideoPlatform {
    YouTube,
    TikTok,
}

/// Video metadata from supported platforms
#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub title: String,
    pub duration: u64,
    pub uploader: String,
    pub platform: VideoPlatform,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported_video_url() {
        // YouTube URLs
        assert!(VideoProcessor::is_supported_video_url(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(VideoProcessor::is_supported_video_url(
            "https://youtu.be/dQw4w9WgXcQ"
        ));
        assert!(VideoProcessor::is_supported_video_url(
            "https://www.youtube.com/shorts/abc123"
        ));
        assert!(VideoProcessor::is_supported_video_url(
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ"
        ));

        // TikTok URLs
        assert!(VideoProcessor::is_supported_video_url(
            "https://www.tiktok.com/@user/video/1234567890"
        ));
        assert!(VideoProcessor::is_supported_video_url(
            "https://vm.tiktok.com/abc123"
        ));
        assert!(VideoProcessor::is_supported_video_url(
            "https://vt.tiktok.com/abc123"
        ));
        assert!(VideoProcessor::is_supported_video_url(
            "https://tiktok.com/t/abc123"
        ));
        assert!(VideoProcessor::is_supported_video_url(
            "https://m.tiktok.com/@user/video/1234567890"
        ));

        // Invalid URLs
        assert!(!VideoProcessor::is_supported_video_url(
            "https://www.example.com"
        ));
        assert!(!VideoProcessor::is_supported_video_url(
            "https://www.instagram.com/p/abc123"
        ));
        assert!(!VideoProcessor::is_supported_video_url(""));
    }

    #[test]
    fn test_detect_platform() {
        // YouTube
        assert_eq!(
            VideoProcessor::detect_platform("https://www.youtube.com/watch?v=abc"),
            VideoPlatform::YouTube
        );
        assert_eq!(
            VideoProcessor::detect_platform("https://youtu.be/abc"),
            VideoPlatform::YouTube
        );

        // TikTok
        assert_eq!(
            VideoProcessor::detect_platform("https://www.tiktok.com/@user/video/123"),
            VideoPlatform::TikTok
        );
        assert_eq!(
            VideoProcessor::detect_platform("https://vm.tiktok.com/abc"),
            VideoPlatform::TikTok
        );

        // Default fallback
        assert_eq!(
            VideoProcessor::detect_platform("https://example.com"),
            VideoPlatform::YouTube
        );
    }

    #[test]
    fn test_get_user_friendly_error() {
        let tiktok_url = "https://www.tiktok.com/@user/video/123";
        let youtube_url = "https://www.youtube.com/watch?v=abc";

        // Test TikTok authentication error
        let auth_error = "Log in for access. Use --cookies-from-browser";
        let result = VideoProcessor::get_user_friendly_error(auth_error, tiktok_url);
        assert!(result.contains("TikTok video requires login"));
        assert!(result.contains("age-restricted"));

        // Test age-restricted content
        let age_error = "not comfortable for some audiences";
        let result = VideoProcessor::get_user_friendly_error(age_error, tiktok_url);
        assert!(result.contains("age-restricted"));

        // Test private video
        let private_error = "Private video";
        let result = VideoProcessor::get_user_friendly_error(private_error, youtube_url);
        assert!(result.contains("private or unavailable"));

        // Test generic TikTok error
        let generic_error = "Some other error";
        let result = VideoProcessor::get_user_friendly_error(generic_error, tiktok_url);
        assert!(result.contains("TikTok video"));
        assert!(result.contains("public, non-restricted"));
    }
}
