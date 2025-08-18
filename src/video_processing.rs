use std::process::Command;
use tokio::process::Command as AsyncCommand;
use crate::errors::AppError;

pub struct VideoProcessor;

impl VideoProcessor {
    /// Process a video file to add caption overlay using ffmpeg
    /// Returns the path to the processed video file
    pub async fn add_caption_overlay(
        input_path: &str,
        output_path: &str,
        caption: &str,
    ) -> Result<(), AppError> {
        // Escape caption text for ffmpeg
        let escaped_caption = escape_ffmpeg_text(caption);
        
        // Build ffmpeg command with font styling matching the web template
        // Font: Impact, size: 55px equivalent, color: #ddd, shadow: 2px 2px 4px rgba(0,0,0,0.5)
        let filter_complex = format!(
            "drawtext=text='{}':fontfile=/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf:fontsize=55:fontcolor=white:x=(w-text_w)/2:y=h-text_h-50:shadowcolor=black:shadowx=2:shadowy=2",
            escaped_caption
        );

        // Try with Impact font first, fallback to Liberation Sans Bold
        let mut cmd = AsyncCommand::new("ffmpeg");
        cmd.args([
            "-i", input_path,
            "-vf", &filter_complex,
            "-c:a", "copy", // Copy audio without re-encoding
            "-y", // Overwrite output file
            output_path,
        ]);

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
            return Self::add_caption_overlay_fallback(input_path, output_path, caption).await;
        }

        tracing::info!("Video processing completed successfully");
        Ok(())
    }

    /// Fallback method using system default font
    async fn add_caption_overlay_fallback(
        input_path: &str,
        output_path: &str,
        caption: &str,
    ) -> Result<(), AppError> {
        let escaped_caption = escape_ffmpeg_text(caption);
        
        // Simpler filter without specific font file
        let filter_complex = format!(
            "drawtext=text='{}':fontsize=55:fontcolor=white:x=(w-text_w)/2:y=h-text_h-50:shadowcolor=black:shadowx=2:shadowy=2",
            escaped_caption
        );

        let mut cmd = AsyncCommand::new("ffmpeg");
        cmd.args([
            "-i", input_path,
            "-vf", &filter_complex,
            "-c:a", "copy",
            "-y",
            output_path,
        ]);

        let output = cmd.output().await.map_err(|e| {
            tracing::error!("Failed to execute ffmpeg fallback: {}", e);
            AppError::IoError(e)
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::error!("FFmpeg fallback failed: {}", stderr);
            return Err(AppError::IoError(std::io::Error::other(
                format!("FFmpeg processing failed: {}", stderr),
            )));
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
        assert_eq!(escape_ffmpeg_text("Hello, World; Test"), "Hello\\, World\\; Test");
    }

    #[test]
    fn test_generate_output_filename() {
        let result = VideoProcessor::generate_output_filename("test.mp4");
        assert!(result.starts_with("test_captioned_"));
        assert!(result.ends_with(".mp4"));
        
        let result = VideoProcessor::generate_output_filename("video");
        assert!(result.starts_with("video_captioned_"));
    }
}