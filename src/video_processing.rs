use crate::errors::AppError;
use serde_json::Value;
use std::process::Command;
use tokio::process::Command as AsyncCommand;

pub struct VideoProcessor;

impl VideoProcessor {
    /// Process a video file to add caption overlay using ffmpeg
    /// Returns the path to the processed video file
    pub async fn add_caption_overlay(
        input_path: &str,
        output_path: &str,
        caption: &str,
    ) -> Result<(), AppError> {
        // Get video dimensions first
        let video_info = Self::get_video_info(input_path).await?;

        // Escape caption text for ffmpeg
        let escaped_caption = escape_ffmpeg_text(caption);

        // Calculate font size based on video resolution
        let font_size = Self::calculate_font_size(video_info.width, video_info.height);
        let shadow_offset = (font_size as f32 * 0.04).max(1.0) as u32; // 4% of font size, minimum 1px
        let bottom_margin = font_size + 20; // Font size + some padding

        tracing::info!(
            "Video resolution: {}x{}, calculated font size: {}",
            video_info.width,
            video_info.height,
            font_size
        );

        // Wrap text to fit within video width
        let wrapped_caption = Self::wrap_text(&escaped_caption, video_info.width, font_size);

        // Check for hardware acceleration
        let (use_hw_accel, hw_accel_args, filter_prefix, filter_suffix, video_codec) = if Self::is_cuda_available() {
            tracing::info!("CUDA detected, using GPU acceleration");
            (
                true,
                vec!["-hwaccel", "cuda", "-hwaccel_output_format", "cuda"],
                "hwupload_cuda,",
                ",hwdownload",
                "h264_nvenc"
            )
        } else if Self::is_vaapi_available() {
            tracing::info!("VAAPI detected, using GPU acceleration");
            (
                true,
                vec!["-vaapi_device", "/dev/dri/renderD128", "-hwaccel", "vaapi", "-hwaccel_output_format", "vaapi"],
                "hwupload,",
                "",
                "h264_vaapi"
            )
        } else {
            tracing::info!("No hardware acceleration available, using CPU processing");
            (false, vec![], "", "", "libx264")
        };

        // Build ffmpeg command with dynamic font sizing and wrapped text
        let filter_complex = format!(
            "{}drawtext=text='{}':fontfile=/usr/share/fonts/truetype/wintc/impact.ttf:fontsize={}:fontcolor=white:x=(w-text_w)/2:y=h-text_h-{}:shadowcolor=black:shadowx={}:shadowy={}:line_spacing=5{}",
            filter_prefix, wrapped_caption, font_size, bottom_margin, shadow_offset, shadow_offset, filter_suffix
        );

        // Try with Impact font first, fallback to Liberation Sans Bold
        let mut cmd = AsyncCommand::new("ffmpeg");
        
        // Build arguments correctly
        let mut args = vec!["-i", input_path];
        
        // Add hardware acceleration args if available (as input options)
        if use_hw_accel {
            args.extend_from_slice(&hw_accel_args);
        }
        
        // Add processing args
        args.extend(&[
            "-vf",
            &filter_complex,
            "-c:a",
            "copy", // Copy audio without re-encoding
            "-c:v",
            video_codec,
            "-preset", 
            "fast", // Faster encoding
            "-y",   // Overwrite output file
            output_path,
        ]);
        
        cmd.args(args);

        tracing::info!("Processing video with caption: {}", caption);
        tracing::debug!("FFmpeg command: {:?}", cmd);

        let output = cmd.output().await.map_err(|e| {
            tracing::error!("Failed to execute ffmpeg: {}", e);
            AppError::IoError(e)
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("FFmpeg failed: {}", stderr);

            // Try fallback with system default font
            return Self::add_caption_overlay_fallback(
                input_path,
                output_path,
                caption,
                font_size,
                shadow_offset,
                bottom_margin,
            )
            .await;
        }

        tracing::info!("Video processing completed successfully");
        Ok(())
    }

    /// Fallback method using system default font
    async fn add_caption_overlay_fallback(
        input_path: &str,
        output_path: &str,
        caption: &str,
        font_size: u32,
        shadow_offset: u32,
        bottom_margin: u32,
    ) -> Result<(), AppError> {
        let escaped_caption = escape_ffmpeg_text(caption);

        // Simpler filter without specific font file but with dynamic sizing and text wrapping
        let filter_complex = format!(
            "drawtext=text='{}':fontsize={}:fontcolor=white:x=(w-text_w)/2:y=h-text_h-{}:shadowcolor=black:shadowx={}:shadowy={}:line_spacing=5",
            escaped_caption, font_size, bottom_margin, shadow_offset, shadow_offset
        );

        let mut cmd = AsyncCommand::new("ffmpeg");
        
        // Base arguments - just input file (no hardware acceleration in fallback)
        let args = vec![
            "-i", input_path,
            "-vf", &filter_complex,
            "-c:a", "copy",
            "-c:v", "libx264", // Always use software encoder in fallback
            "-preset", "fast",
            "-y", output_path,
        ];
        
        cmd.args(args);

        let output = cmd.output().await.map_err(|e| {
            tracing::error!("Failed to execute ffmpeg fallback: {}", e);
            AppError::IoError(e)
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("FFmpeg fallback failed: {}", stderr);
            return Err(AppError::IoError(std::io::Error::other(format!(
                "FFmpeg processing failed: {}",
                stderr
            ))));
        }

        tracing::info!("Video processing completed with fallback font");
        Ok(())
    }

    /// Check if ffmpeg is available on the system
    pub fn is_ffmpeg_available() -> bool {
        Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Check if CUDA is available on the system
    pub fn is_cuda_available() -> bool {
        // Check if nvidia-smi is available and working
        let nvidia_smi_check = Command::new("nvidia-smi")
            .arg("--query-gpu=name")
            .arg("--format=csv")
            .output()
            .map(|output| output.status.success() && !output.stdout.is_empty())
            .unwrap_or(false);

        if nvidia_smi_check {
            // Also check if ffmpeg supports cuda
            let ffmpeg_cuda_check = Command::new("ffmpeg")
                .arg("-hwaccels")
                .output()
                .map(|output| {
                    output.status.success() && 
                    String::from_utf8_lossy(&output.stdout).contains("cuda")
                })
                .unwrap_or(false);
            
            tracing::info!("CUDA availability: nvidia-smi={}, ffmpeg-cuda={}", nvidia_smi_check, ffmpeg_cuda_check);
            return nvidia_smi_check && ffmpeg_cuda_check;
        }
        
        false
    }

    /// Check if VAAPI is available on the system (Intel/AMD GPU acceleration)
    pub fn is_vaapi_available() -> bool {
        // Check if vainfo is available and working
        let vaapi_check = Command::new("vainfo")
            .arg("--display")
            .arg("drm")
            .arg("--device")
            .arg("/dev/dri/card0")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if vaapi_check {
            // Also check if ffmpeg supports vaapi
            let ffmpeg_vaapi_check = Command::new("ffmpeg")
                .arg("-hwaccels")
                .output()
                .map(|output| {
                    output.status.success() && 
                    String::from_utf8_lossy(&output.stdout).contains("vaapi")
                })
                .unwrap_or(false);
            
            tracing::info!("VAAPI availability: vainfo={}, ffmpeg-vaapi={}", vaapi_check, ffmpeg_vaapi_check);
            return vaapi_check && ffmpeg_vaapi_check;
        }
        
        false
    }

    /// Check if yt-dlp is available on the system
    pub fn is_ytdlp_available() -> bool {
        Command::new("yt-dlp")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Download video from supported platforms (YouTube, TikTok) and process it with caption if provided
    pub async fn download_and_process_video(url: &str, output_dir: &str, caption: Option<&str>) -> Result<String, AppError> {
        // Validate video URL
        if !Self::is_supported_video_url(url) {
            return Err(AppError::IoError(std::io::Error::other(
                "Invalid video URL. Supported platforms: YouTube, TikTok",
            )));
        }

        // Check if yt-dlp is available
        if !Self::is_ytdlp_available() {
            return Err(AppError::IoError(std::io::Error::other(
                "yt-dlp is not available on the system",
            )));
        }

        // Create output directory
        tokio::fs::create_dir_all(output_dir).await.map_err(|e| {
            tracing::error!("Failed to create output directory: {}", e);
            AppError::IoError(e)
        })?;

        // Generate unique filename
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        
        // For streaming, we'll use a temporary name
        let temp_filename = format!("temp_video_{}.mp4", timestamp);
        let temp_path = format!("{}/{}", output_dir, temp_filename);

        // Download video with yt-dlp directly to MP4 format for better compatibility
        let mut cmd = AsyncCommand::new("yt-dlp");
        cmd.args([
            "--cookies-from-browser",
            "firefox", // Use Firefox cookies for authentication
            "--format",
            "mp4[height<=720]/mp4/best[height<=720]/best", // Prefer mp4, limit to 720p
            "--output",
            &temp_path, // Direct output to our temp file
            "--no-playlist", // Only download single video
            url,
        ]);

        tracing::info!("Downloading and converting video: {}", url);
        tracing::debug!("yt-dlp command: {:?}", cmd);

        let output = cmd.output().await.map_err(|e| {
            tracing::error!("Failed to execute yt-dlp: {}", e);
            AppError::IoError(e)
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("yt-dlp failed: {}", stderr);

            // Clean up temp file if it was created
            tokio::task::spawn_blocking(move || {
                let _ = std::fs::remove_file(&temp_path);
            });

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

            return Err(AppError::IoError(std::io::Error::other(format!(
                "Video download failed: {}",
                stderr
            ))));
        }

        // Check if the temp file was created
        if tokio::fs::metadata(&temp_path).await.is_err() {
            return Err(AppError::IoError(std::io::Error::other(
                "Downloaded video file not found",
            )));
        }

        tracing::info!("Successfully downloaded video to: {}", temp_path);

        // If caption is provided, process the video with caption overlay
        if let Some(caption_text) = caption {
            if !caption_text.trim().is_empty() {
                tracing::info!("Processing video with caption overlay");
                
                // Generate output filename
                let output_filename = format!("video_{}_captioned.mp4", timestamp);
                let output_path = format!("{}/{}", output_dir, output_filename);
                
                // Process video with caption
                match Self::add_caption_overlay(&temp_path, &output_path, caption_text).await {
                    Ok(_) => {
                        // Remove temporary file
                        tokio::task::spawn_blocking(move || {
                            let _ = std::fs::remove_file(&temp_path);
                        });
                        tracing::info!("Video processing completed: {}", output_path);
                        return Ok(output_filename);
                    }
                    Err(e) => {
                        // Clean up files on error
                        tokio::task::spawn_blocking(move || {
                            let _ = std::fs::remove_file(&temp_path);
                            let _ = std::fs::remove_file(&output_path);
                        });
                        return Err(e);
                    }
                }
            }
        }

        // No caption processing needed, rename temp file to final name
        let final_filename = format!("video_{}.mp4", timestamp);
        let final_path = format!("{}/{}", output_dir, final_filename);
        
        tokio::fs::rename(&temp_path, &final_path).await.map_err(|e| {
            tracing::error!("Failed to rename video file: {}", e);
            // Clean up temp file in a blocking context
            tokio::task::spawn_blocking(move || {
                let _ = std::fs::remove_file(&temp_path);
            });
            AppError::IoError(e)
        })?;

        tracing::info!("Video download completed: {}", final_path);
        Ok(final_filename)
    }

    /// Stream video download directly to processing (most efficient approach)
    pub async fn stream_process_video(url: &str, output_dir: &str, caption: Option<&str>) -> Result<String, AppError> {
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

        if !Self::is_ffmpeg_available() {
            return Err(AppError::IoError(std::io::Error::other(
                "ffmpeg is not available on the system",
            )));
        }

        // Create output directory
        tokio::fs::create_dir_all(output_dir).await.map_err(|e| {
            tracing::error!("Failed to create output directory: {}", e);
            AppError::IoError(e)
        })?;

        // Generate unique filename
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let output_filename = if caption.is_some() && !caption.unwrap().trim().is_empty() {
            format!("video_{}_captioned.mp4", timestamp)
        } else {
            format!("video_{}.mp4", timestamp)
        };
        let output_path = format!("{}/{}", output_dir, output_filename);

        // First, download the video using yt-dlp
        tracing::info!("Downloading video: {}", url);
        
        let mut download_cmd = AsyncCommand::new("yt-dlp");
        download_cmd.args([
            "--cookies-from-browser",
            "firefox",
            "--format",
            "mp4[height<=720]/mp4/best[height<=720]/best",
            "--output",
            &output_path,
            "--no-playlist",
            url,
        ]);

        let download_output = download_cmd.output().await.map_err(|e| {
            tracing::error!("Failed to execute yt-dlp: {}", e);
            AppError::IoError(e)
        })?;

        if !download_output.status.success() {
            let stderr = String::from_utf8_lossy(&download_output.stderr);
            tracing::error!("yt-dlp download failed: {}", stderr);

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

            return Err(AppError::IoError(std::io::Error::other(format!(
                "Video download failed: {}",
                stderr
            ))));
        }

        // Check if the file was created
        if tokio::fs::metadata(&output_path).await.is_err() {
            return Err(AppError::IoError(std::io::Error::other(
                "Downloaded video file not found",
            )));
        }

        tracing::info!("Video downloaded successfully: {}", output_path);

        // If caption is provided, process the video with caption overlay
        if let Some(caption_text) = caption {
            if !caption_text.trim().is_empty() {
                tracing::info!("Processing video with caption overlay");
                
                // Generate processed filename
                let processed_filename = format!("video_{}_captioned_final.mp4", timestamp);
                let processed_path = format!("{}/{}", output_dir, processed_filename);
                
                // Process video with caption
                match Self::add_caption_overlay(&output_path, &processed_path, caption_text).await {
                    Ok(_) => {
                        // Remove original file to save space
                        let _ = tokio::fs::remove_file(&output_path).await;
                        tracing::info!("Video processing completed: {}", processed_path);
                        return Ok(processed_filename);
                    }
                    Err(e) => {
                        // Clean up files on error
                        let _ = tokio::fs::remove_file(&output_path).await;
                        let _ = tokio::fs::remove_file(&processed_path).await;
                        return Err(e);
                    }
                }
            }
        }

        tracing::info!("Video processing completed successfully: {}", output_path);
        Ok(output_filename)
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
            "--cookies-from-browser",
            "firefox", // Use Firefox cookies for authentication
            "--dump-json",
            "--no-playlist",
            url,
        ]);

        let output = cmd.output().await.map_err(|e| {
            tracing::error!("Failed to execute yt-dlp for info: {}", e);
            AppError::IoError(e)
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

            return Err(AppError::IoError(std::io::Error::other(format!(
                "Video info extraction failed: {}",
                stderr
            ))));
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

    /// Get video information (width, height, duration)
    async fn get_video_info(input_path: &str) -> Result<VideoInfo, AppError> {
        let mut cmd = AsyncCommand::new("ffprobe");
        cmd.args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            input_path,
        ]);

        let output = cmd.output().await.map_err(|e| {
            tracing::error!("Failed to execute ffprobe: {}", e);
            AppError::IoError(e)
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("ffprobe failed: {}", stderr);
            return Err(AppError::IoError(std::io::Error::other(format!(
                "Failed to get video info: {}",
                stderr
            ))));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let json: Value = serde_json::from_str(&json_str).map_err(|e| {
            tracing::error!("Failed to parse ffprobe JSON output: {}", e);
            AppError::IoError(std::io::Error::other("Failed to parse video information"))
        })?;

        // Find the video stream
        let streams = json["streams"]
            .as_array()
            .ok_or_else(|| AppError::IoError(std::io::Error::other("No streams found in video")))?;

        for stream in streams {
            if stream["codec_type"].as_str() == Some("video") {
                let width = stream["width"].as_u64().unwrap_or(1920) as u32;
                let height = stream["height"].as_u64().unwrap_or(1080) as u32;

                return Ok(VideoInfo { width, height });
            }
        }

        // Fallback to common resolution if no video stream found
        Ok(VideoInfo {
            width: 1920,
            height: 1080,
        })
    }

    /// Calculate appropriate font size based on video resolution
    /// Uses a scaling formula that considers both width and height
    fn calculate_font_size(width: u32, height: u32) -> u32 {
        // Base font size for 1920x1080 (Full HD) is 55px
        let base_width = 1920.0;
        let base_height = 1080.0;
        let base_font_size = 75.0;

        // Calculate scaling factor based on video area compared to Full HD
        let video_area = (width * height) as f32;
        let base_area = base_width * base_height;
        let area_scale = (video_area / base_area).sqrt();

        // Apply scaling with some constraints
        let scaled_font_size = base_font_size * area_scale;

        // Clamp font size to reasonable bounds
        let min_font_size = 16.0; // Minimum readable size
        let max_font_size = 120.0; // Maximum to avoid overwhelming small videos

        scaled_font_size.clamp(min_font_size, max_font_size) as u32
    }

    /// Wrap text to fit within video width
    /// Estimates character width and breaks text into multiple lines
    fn wrap_text(text: &str, video_width: u32, font_size: u32) -> String {
        // Rough estimate: each character is about 0.6 * font_size pixels wide
        let char_width = (font_size as f32 * 0.6) as u32;
        let max_chars_per_line = ((video_width as f32 * 0.9) / char_width as f32) as usize;

        if max_chars_per_line == 0 || text.len() <= max_chars_per_line {
            return text.to_string();
        }

        let words: Vec<&str> = text.split_whitespace().collect();
        let mut lines = Vec::new();
        let mut current_line = String::new();

        for word in words {
            let test_line = if current_line.is_empty() {
                word.to_string()
            } else {
                format!("{} {}", current_line, word)
            };

            if test_line.len() <= max_chars_per_line {
                current_line = test_line;
            } else {
                if !current_line.is_empty() {
                    lines.push(current_line);
                }
                current_line = word.to_string();
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }

        lines.join("\n")
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

    /// Generate a unique output filename for processed video
    pub fn generate_output_filename(original_filename: &str) -> String {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(dot_pos) = original_filename.rfind('.') {
            let (name, ext) = original_filename.split_at(dot_pos);
            format!("{}_captioned_{}{}", name, timestamp, ext)
        } else {
            format!("{}_captioned_{}", original_filename, timestamp)
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

/// Video information for processing
#[derive(Debug, Clone)]
struct VideoInfo {
    pub width: u32,
    pub height: u32,
}

/// Escape special characters in text for ffmpeg drawtext filter
fn escape_ffmpeg_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace(':', "\\:")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace(',', "\\,")
        .replace(';', "\\;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_ffmpeg_text() {
        assert_eq!(escape_ffmpeg_text("Hello World"), "Hello World");
        assert_eq!(escape_ffmpeg_text("Hello: World"), "Hello\\: World");
        assert_eq!(escape_ffmpeg_text("Hello [World]"), "Hello \\[World\\]");
        assert_eq!(
            escape_ffmpeg_text("Hello, World; Test"),
            "Hello\\, World\\; Test"
        );
    }

    #[test]
    fn test_generate_output_filename() {
        let result = VideoProcessor::generate_output_filename("test.mp4");
        assert!(result.starts_with("test_captioned_"));
        assert!(result.ends_with(".mp4"));

        let result = VideoProcessor::generate_output_filename("video");
        assert!(result.starts_with("video_captioned_"));
    }

    #[test]
    fn test_wrap_text() {
        // Short text should not be wrapped
        let result = VideoProcessor::wrap_text("Hello World", 1920, 50);
        assert_eq!(result, "Hello World");

        // Long text should be wrapped
        let long_text = "This is a very long caption that should be wrapped into multiple lines";
        let result = VideoProcessor::wrap_text(long_text, 360, 30);
        assert!(result.contains("\n"));

        // Empty text
        let result = VideoProcessor::wrap_text("", 1920, 50);
        assert_eq!(result, "");
    }

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
