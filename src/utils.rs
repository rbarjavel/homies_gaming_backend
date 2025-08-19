use std::path::{Component, Path, PathBuf};

/// Sanitize a filename to prevent path traversal attacks
/// This function:
/// 1. Removes any path components (keeps only the file name)
/// 2. Resolves any ".." components
/// 3. Ensures the result is a valid UTF-8 filename
pub fn sanitize_filename(filename: &str) -> Option<String> {
    // Convert to Path and get just the file name component
    let path = Path::new(filename);
    let file_name = path.file_name()?;
    let file_name_str = file_name.to_str()?;

    // Additional sanitization to prevent special characters that could be problematic
    let sanitized = file_name_str
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        .collect::<String>();

    // Ensure we still have a valid filename after sanitization
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        return None;
    }

    Some(sanitized)
}

/// Validate and resolve a file path to prevent directory traversal
/// This function ensures paths are within a specified base directory
pub fn validate_file_path(base_dir: &str, user_path: &str) -> Option<String> {
    let base = Path::new(base_dir);
    let path = Path::new(user_path);
    
    // Handle empty path case
    if user_path.is_empty() {
        return base.to_str().map(|s| s.to_string());
    }
    
    // Track our virtual position - we start at the virtual root (0)
    let mut virtual_depth = 0;
    let mut resolved_path = PathBuf::new();
    
    // Process each component
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {
                // Skip root and prefix components to prevent absolute paths
                continue;
            }
            Component::CurDir => {
                // Skip current directory references
                continue;
            }
            Component::ParentDir => {
                // Move up one level, but not beyond the base directory
                if virtual_depth > 0 {
                    resolved_path.pop();
                    virtual_depth -= 1;
                }
                // If we're at the virtual root (virtual_depth == 0), 
                // we ignore the .. component entirely
            }
            Component::Normal(component) => {
                // Add normal components
                resolved_path.push(component);
                virtual_depth += 1;
            }
        }
    }
    
    // Combine with base directory
    let full_path = base.join(resolved_path);
    
    // Ensure the resolved path is within the base directory
    if full_path.starts_with(base) {
        full_path.to_str().map(|s| s.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        // Valid filenames
        assert_eq!(sanitize_filename("test.mp4"), Some("test.mp4".to_string()));
        assert_eq!(sanitize_filename("my file.jpg"), Some("my file.jpg".to_string()));
        
        // Filenames with path components (should be stripped)
        assert_eq!(sanitize_filename("../test.mp4"), Some("test.mp4".to_string()));
        assert_eq!(sanitize_filename("/etc/passwd"), Some("passwd".to_string()));
        assert_eq!(sanitize_filename("folder/test.mp4"), Some("test.mp4".to_string()));
        
        // Invalid filenames
        assert_eq!(sanitize_filename(""), None);
        assert_eq!(sanitize_filename("."), None);
        assert_eq!(sanitize_filename(".."), None);
        
        // Filenames with special characters
        assert_eq!(sanitize_filename("test<file>.mp4"), Some("testfile.mp4".to_string()));
    }

    #[test]
    fn test_validate_file_path() {
        // Valid paths
        assert_eq!(
            validate_file_path("uploads", "test.mp4"), 
            Some("uploads/test.mp4".to_string())
        );
        
        // Path traversal attempts (should be contained within base)
        assert_eq!(
            validate_file_path("uploads", "../etc/passwd"), 
            Some("uploads/etc/passwd".to_string())
        );
        
        assert_eq!(
            validate_file_path("uploads", "folder/../../../etc/passwd"), 
            Some("uploads/etc/passwd".to_string())
        );
        
        // Empty or invalid paths
        assert_eq!(validate_file_path("uploads", ""), Some("uploads".to_string()));
    }
}