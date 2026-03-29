//! Logging initialization helpers.
//!
//! Binaries use this to route `log`/`env_logger` output into a file under
//! `logs/` in the current working directory.

use std::backtrace::Backtrace;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use env_logger::{Builder, Env, Target};

/// Initialize env_logger to append to `logs/<name>.log`.
///
/// Honors `RUST_LOG` and defaults to `info` when the variable is unset.
/// Returns the resolved log file path on success.
pub fn init_file_logger(name: &str) -> io::Result<PathBuf> {
    let mut log_dir = std::env::current_dir()?;
    log_dir.push("logs");
    fs::create_dir_all(&log_dir)?;

    let mut log_path = log_dir;
    log_path.push(format!("{name}.log"));

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    // Suppress wgpu_hal's per-frame "Suboptimal present" warnings which
    // flood the log (~100K+ lines per session on AMD Vulkan).
    let env = Env::default().default_filter_or("info,wgpu_hal=error");
    let mut builder = Builder::from_env(env);
    builder.format_timestamp_secs();
    builder.target(Target::Pipe(Box::new(file)));
    builder.init();

    Ok(log_path)
}

/// Register a panic hook that writes the panic info and backtrace to the log
/// file. The default stderr hook is preserved so terminal users still see output.
pub fn install_panic_hook(log_path: &Path) {
    let prev_hook = std::panic::take_hook();
    let log_path = log_path.to_owned();

    std::panic::set_hook(Box::new(move |info| {
        // Capture while the panic stack is still live.
        let backtrace = Backtrace::force_capture();

        if let Ok(mut file) = OpenOptions::new().append(true).open(&log_path) {
            let _ = writeln!(file, "\n========== PANIC ==========");
            let _ = writeln!(file, "{info}");
            let _ = writeln!(file, "\n{backtrace}");
            let _ = writeln!(file, "===========================");
            let _ = file.flush();
        }

        // Preserve default stderr output for terminal users.
        prev_hook(info);
    }));
}
