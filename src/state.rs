use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::time::{SystemTime, Duration};

#[derive(Clone, Debug)]
pub struct MediaInfo {
    pub filename: String,
    pub media_type: MediaType,
    pub upload_time: SystemTime,
    pub marked_for_deletion: bool,
}

#[derive(Clone, Debug)]
pub enum MediaType {
    Image,
    Video,
}

pub struct MediaViewState {
    last_media: Option<MediaInfo>,
    viewed_by: HashMap<String, HashSet<IpAddr>>, // filename -> set of IPs that viewed it
}

impl MediaViewState {
    pub fn new() -> Self {
        Self {
            last_media: None,
            viewed_by: HashMap::new(),
        }
    }

    pub fn set_last_media(&mut self, media: MediaInfo) {
        self.last_media = Some(media);
    }

    pub fn mark_viewed(&mut self, filename: &str, ip: IpAddr) -> bool {
        let viewed_set = self.viewed_by.entry(filename.to_string()).or_insert_with(HashSet::new);
        viewed_set.insert(ip)
        // Returns true if IP was newly inserted (first view), false if already existed
    }

    pub fn get_last_media(&self) -> Option<&MediaInfo> {
        self.last_media.as_ref()
    }

    pub fn has_been_viewed(&self, filename: &str, ip: IpAddr) -> bool {
        self.viewed_by
            .get(filename)
            .map(|viewed_set| viewed_set.contains(&ip))
            .unwrap_or(false)
    }

    pub fn get_last_media_for_ip(&self, ip: IpAddr) -> Option<&MediaInfo> {
        if let Some(media) = &self.last_media {
            // If IP hasn't viewed this media yet and it's not marked for deletion, return it
            if !self.has_been_viewed(&media.filename, ip) && !media.marked_for_deletion {
                return Some(media);
            }
        }
        None
    }

    // Mark file for deletion
    pub fn mark_for_deletion(&mut self, filename: &str) {
        if let Some(media) = &mut self.last_media {
            if media.filename == filename {
                media.marked_for_deletion = true;
            }
        }
    }
    
    // Completely remove file from state (for re-upload)
    pub fn remove_file_from_state(&mut self, filename: &str) {
        // Remove from last_media if it matches
        if let Some(media) = &self.last_media {
            if media.filename == filename {
                self.last_media = None;
            }
        }
        // Remove from viewed_by tracking
        self.viewed_by.remove(filename);
    }
    
    pub fn get_files_to_delete(&self, threshold: Duration) -> Vec<String> {
        let now = SystemTime::now();
        let mut files = Vec::new();
        
        if let Some(media) = &self.last_media {
            if let Ok(elapsed) = now.duration_since(media.upload_time) {
                if elapsed > threshold && !media.marked_for_deletion {
                    files.push(media.filename.clone());
                }
            }
        }
        
        files
    }
    
    // Check if a file exists in our state
    pub fn file_exists(&self, filename: &str) -> bool {
        if let Some(media) = &self.last_media {
            media.filename == filename && !media.marked_for_deletion
        } else {
            false
        }
    }
}
