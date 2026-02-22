//! Unix domain socket listener for external menu commands.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::mpsc::Sender;

use log::{debug, info, warn};

#[derive(Debug)]
pub enum SocketCommand {
    MenuToggle,
    MenuUp,
    MenuDown,
    MenuSelect,
    MenuBack,
    Popup { title: String, description: String },
}

/// Listen for commands on a Unix domain socket. Blocks forever.
pub fn listen(path: &str, tx: Sender<SocketCommand>) -> Result<(), String> {
    // Remove stale socket
    if Path::new(path).exists() {
        let _ = std::fs::remove_file(path);
    }

    let listener = UnixListener::bind(path).map_err(|e| format!("bind {path}: {e}"))?;
    info!("Socket listening at {path}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let reader = BufReader::new(stream);
                for line in reader.lines() {
                    match line {
                        Ok(line) => {
                            if let Some(cmd) = parse_command(&line) {
                                debug!("Socket command: {line}");
                                if tx.send(cmd).is_err() {
                                    return Err("channel closed".into());
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Socket read error: {e}");
                            break;
                        }
                    }
                }
            }
            Err(e) => warn!("Socket accept error: {e}"),
        }
    }

    Ok(())
}

fn parse_command(line: &str) -> Option<SocketCommand> {
    match line.trim() {
        "MENU_TOGGLE" => Some(SocketCommand::MenuToggle),
        "MENU_UP" => Some(SocketCommand::MenuUp),
        "MENU_DOWN" => Some(SocketCommand::MenuDown),
        "MENU_SELECT" => Some(SocketCommand::MenuSelect),
        "MENU_BACK" => Some(SocketCommand::MenuBack),
        s if s.starts_with("POPUP ") => {
            let rest = &s[6..];
            let mut parts = rest.splitn(2, '|');
            Some(SocketCommand::Popup {
                title: parts.next().unwrap_or("").to_string(),
                description: parts.next().unwrap_or("").to_string(),
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_menu_commands() {
        assert!(matches!(parse_command("MENU_TOGGLE"), Some(SocketCommand::MenuToggle)));
        assert!(matches!(parse_command("MENU_UP"), Some(SocketCommand::MenuUp)));
        assert!(matches!(parse_command("MENU_DOWN"), Some(SocketCommand::MenuDown)));
        assert!(matches!(parse_command("MENU_SELECT"), Some(SocketCommand::MenuSelect)));
        assert!(matches!(parse_command("MENU_BACK"), Some(SocketCommand::MenuBack)));
    }

    #[test]
    fn parse_popup_command() {
        if let Some(SocketCommand::Popup { title, description }) = parse_command("POPUP First Blood|Defeat the boss") {
            assert_eq!(title, "First Blood");
            assert_eq!(description, "Defeat the boss");
        } else {
            panic!("expected Popup");
        }
    }

    #[test]
    fn parse_unknown_returns_none() {
        assert!(parse_command("GARBAGE").is_none());
        assert!(parse_command("").is_none());
    }
}
