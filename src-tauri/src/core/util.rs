//! Small shared helpers.

use std::path::Path;
use std::process::Command;

/// Build a [`Command`] that never flashes a console window on Windows.
///
/// FFmpeg/ffprobe are console subprocesses; without this flag each invocation
/// pops a black window in a GUI app. `CREATE_NO_WINDOW = 0x0800_0000`.
pub fn command_no_window(program: &Path) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Human-readable byte count (e.g. "1.5 GB").
pub fn human_bytes(n: f64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    if n <= 0.0 {
        return "0 B".into();
    }
    let mut size = n;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", size as u64, UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes() {
        assert_eq!(human_bytes(0.0), "0 B");
        assert_eq!(human_bytes(512.0), "512 B");
        assert_eq!(human_bytes(1024.0), "1.0 KB");
        assert_eq!(human_bytes(1_572_864.0), "1.5 MB");
    }
}
