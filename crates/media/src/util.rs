use std::process::Output;

use tokio::process::Command;

use crate::error::MediaError;

/// `kill_on_drop` so a cancelled task doesn't leave an orphaned ffmpeg behind.
pub(crate) async fn run_tool(binary: &str, mut cmd: Command) -> Result<Output, MediaError> {
    cmd.kill_on_drop(true);
    let out = cmd.output().await.map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => MediaError::BinaryNotFound(binary.to_string()),
        _ => MediaError::Io(e),
    })?;
    if !out.status.success() {
        return Err(MediaError::ToolFailed {
            binary: binary.to_string(),
            code: out.status.code().unwrap_or(-1),
            stderr: truncate_utf8(&out.stderr, 4000),
        });
    }
    Ok(out)
}

// ffmpeg stderr can be megabytes.
pub(crate) fn truncate_utf8(bytes: &[u8], max: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    if s.len() <= max {
        return s.into_owned();
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}\u{2026}", &s[..cut])
}
