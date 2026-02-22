//! Minimal sound playback via aplay (Batocera/ALSA).

use std::path::Path;

use log::debug;

/// Play a WAV file asynchronously. Silently skips if file missing or aplay unavailable.
pub fn play_sound(path: &Path) {
    if !path.exists() {
        return;
    }
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        debug!("Playing sound: {}", path.display());
        let _ = std::process::Command::new("aplay")
            .arg("-q")
            .arg(&path)
            .status();
    });
}
