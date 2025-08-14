use thiserror::Error;
use warp::reject::Reject;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Template rendering error: {0}")]
    RenderError(#[from] askama::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Multipart error")]
    MultipartError,
}

impl Reject for AppError {}
