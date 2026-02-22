//! RetroArch log file watcher.
//! Monitors /tmp/retroarch.log for RetroAchievements events using inotify.

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

use log::{debug, error, info, warn};
use notify::{EventKind, RecursiveMode, Watcher};

#[derive(Debug, Clone)]
pub struct AchievementEvent {
    pub title: String,
    pub description: String,
}

/// Watch the RetroArch log file for achievement events.
/// Blocks the calling thread. Sends parsed events through `tx`.
pub fn watch_log(path: &Path, tx: Sender<AchievementEvent>) -> Result<(), String> {
    // Wait for file to exist
    while !path.exists() {
        info!("Waiting for log file: {}", path.display());
        std::thread::sleep(Duration::from_secs(2));
    }

    let mut file = File::open(path).map_err(|e| format!("open log: {e}"))?;
    // Seek to end — only process new lines
    file.seek(SeekFrom::End(0)).map_err(|e| format!("seek: {e}"))?;
    let mut reader = BufReader::new(file);
    let mut line_buf = String::new();

    // Set up file watcher
    let (notify_tx, notify_rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        if let Ok(event) = res {
            let _ = notify_tx.send(event);
        }
    })
    .map_err(|e| format!("watcher: {e}"))?;

    watcher
        .watch(path, RecursiveMode::NonRecursive)
        .map_err(|e| format!("watch: {e}"))?;

    info!("Watching log file for RCHEEVOS events");

    loop {
        // Wait for file change notification
        match notify_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(event) => {
                if !matches!(event.kind, EventKind::Modify(_)) {
                    continue;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("watcher disconnected".to_string());
            }
        }

        // Read new lines
        loop {
            line_buf.clear();
            match reader.read_line(&mut line_buf) {
                Ok(0) => break, // No more data
                Ok(_) => {
                    if let Some(event) = parse_cheevo_line(&line_buf) {
                        if tx.send(event).is_err() {
                            return Err("channel closed".to_string());
                        }
                    }
                }
                Err(e) => {
                    warn!("Read error: {e}");
                    break;
                }
            }
        }
    }
}

/// Parse a RetroArch log line for achievement unlock events.
/// Format: `[INFO] [RCHEEVOS]: awarding cheevo <ID>: <Name> (<Description>)`
fn parse_cheevo_line(line: &str) -> Option<AchievementEvent> {
    // Look for the RCHEEVOS award pattern
    let marker = "[RCHEEVOS]: awarding cheevo";
    let idx = line.find(marker)?;
    let after = &line[idx + marker.len()..];

    // Skip the ID: find the first `: ` after the number
    let colon_idx = after.find(": ")?;
    let rest = &after[colon_idx + 2..].trim();

    // Split title and description at ` (`
    if let Some(paren_idx) = rest.find(" (") {
        let title = rest[..paren_idx].trim().to_string();
        let desc_end = rest.rfind(')')?;
        let description = rest[paren_idx + 2..desc_end].trim().to_string();
        Some(AchievementEvent { title, description })
    } else {
        // No description in parentheses — use the whole thing as title
        Some(AchievementEvent {
            title: rest.trim_end().to_string(),
            description: String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_standard_cheevo() {
        let line = "[INFO] [RCHEEVOS]: awarding cheevo 12345: First Blood (Defeat the first boss)";
        let event = parse_cheevo_line(line).unwrap();
        assert_eq!(event.title, "First Blood");
        assert_eq!(event.description, "Defeat the first boss");
    }

    #[test]
    fn parse_cheevo_no_description() {
        let line = "[INFO] [RCHEEVOS]: awarding cheevo 99: Welcome";
        let event = parse_cheevo_line(line).unwrap();
        assert_eq!(event.title, "Welcome");
        assert_eq!(event.description, "");
    }

    #[test]
    fn ignore_non_cheevo_line() {
        let line = "[INFO] [RCHEEVOS]: login succeeded";
        assert!(parse_cheevo_line(line).is_none());
    }
}
