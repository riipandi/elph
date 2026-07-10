use std::path::Path;

/// Returns the final path component for display (e.g. `/home/user/elph` → `elph`).
pub fn path_basename(path: &str) -> &str {
    Path::new(path).file_name().and_then(|s| s.to_str()).unwrap_or(path)
}
