//! Small helpers several modules share.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Write .tmp then rename, so a crash never leaves a half-written file.
pub fn atomic_write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// One spelling of a path for comparisons: forward slashes, no trailing slash, lowercased.
pub fn path_key(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

/// A temp folder name no other call in this process uses (the PR poller and a click can run at the same ms):
/// `<temp>/<prefix>-<pid>-<ms>-<n>`. The caller creates and removes it.
pub fn unique_temp_dir(prefix: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    std::env::temp_dir().join(format!("{prefix}-{}-{ms}-{n}", std::process::id()))
}

/// The first 7 characters of a commit id.
pub fn sha7(s: &str) -> &str {
    &s[..s.len().min(7)]
}

/// The last `max` bytes of `s`, starting on a character boundary.
pub fn tail_bytes(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut i = s.len() - max;
    while !s.is_char_boundary(i) {
        i += 1;
    }
    &s[i..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_on_char_boundaries() {
        assert_eq!(tail_bytes("가나다", 4), "다");
        assert_eq!(path_key("C:\\A\\b\\"), "c:/a/b");
        assert_eq!(sha7("abcdef123"), "abcdef1");
        assert_ne!(unique_temp_dir("maestro-t"), unique_temp_dir("maestro-t"));
    }
}
