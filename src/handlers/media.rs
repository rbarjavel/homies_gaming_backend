use warp::{Rejection, Reply};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::net::SocketAddr;
use askama::Template;
use crate::{templates::MediaContentTemplate, state::MediaViewState, errors::AppError};
use std::time::Duration;
use tokio::time::sleep;

pub type SharedState = Arc<RwLock<MediaViewState>>;

pub async fn last_media(addr: Option<SocketAddr>, state: SharedState) -> Result<impl Reply, Rejection> {
    // Get client IP
    sleep(Duration::from_millis(100)).await;
    let client_ip = addr.map(|socket_addr| socket_addr.ip());

    let state_guard = state.read().await;

    // Get media for this IP (only if not viewed yet and not deleted)
    let (media_info, should_mark_viewed) = if let Some(ip) = client_ip {
        if let Some(media) = state_guard.get_last_media_for_ip(ip) {
            (Some(media.clone()), true)
        } else {
            (None, false)
        }
    } else {
        (None, false)
    };

    // If we need to mark as viewed, release read lock and acquire write lock
    if should_mark_viewed {
        if let Some(media) = &media_info {
            let filename = media.filename.clone();
            drop(state_guard); // Release read lock
            let mut state_guard = state.write().await; // Acquire write lock
            if let Some(ip) = client_ip {
                state_guard.mark_viewed(&filename, ip);
            }
        }
    } else {
        drop(state_guard); // Release read lock
    }

    // Render template
    let template = MediaContentTemplate {
        media_info: media_info.as_ref(),
    };

    match template.render() {
        Ok(html) => Ok(warp::reply::html(html)),
        Err(e) => {
            tracing::error!("Template render error: {}", e);
            Err(warp::reject::custom(AppError::RenderError(e)))
        }
    }
}

pub async fn index_page() -> Result<impl Reply, Rejection> {
    use crate::templates::IndexTemplate;
    use askama::Template;
    
    let template = IndexTemplate;
    match template.render() {
        Ok(html) => Ok(warp::reply::html(html)),
        Err(e) => {
            tracing::error!("Template render error: {}", e);
            Err(warp::reject::custom(AppError::RenderError(e)))
        }
    }
}
