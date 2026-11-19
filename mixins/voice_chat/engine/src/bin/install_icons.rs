//! Post-build step: install the mod's icon resource pack.
//!
//! A native `.hnmod` can't carry GUI textures in the atlas layer, so the icons
//! travel inside it as `data/voice_chat-icons.zip` and this step — run by the
//! launcher after the game compiles — hands that pack to `horizon-launcher
//! packs` so it is installed and turned on. One `.hnmod` to add; the icons come
//! with it.
//!
//! It never fails the game build: a problem here only means the HUD shows text
//! without icons, so every error is logged and the step still exits 0.

use std::path::{Path, PathBuf};
use std::process::Command;

const PACK: &str = "voice_chat-icons.zip";

fn main() {
    if let Err(e) = run() {
        eprintln!(
            "voice_chat: icon pack not installed automatically ({e}). \
             You can add it yourself: horizon-launcher packs add <...>/{PACK} && packs on {PACK}"
        );
    }
}

fn run() -> Result<(), String> {
    // The bundle's staged `data/` (read-only) holds the pack.
    let data = std::env::var("HORIZON_NATIVE_BUNDLE_DATA_DIR")
        .map_err(|_| "HORIZON_NATIVE_BUNDLE_DATA_DIR unset".to_string())?;
    let pack = Path::new(&data).join(PACK);
    if !pack.is_file() {
        return Err(format!("{} missing", pack.display()));
    }

    let launcher = find_launcher().ok_or("horizon-launcher not found on PATH")?;

    // `add` never overwrites: given an existing file it installs the pack as
    // "name (2).zip" etc., and the game keeps using the first one — stale
    // icons forever. Remove every copy of our pack first so the fresh one
    // lands under the canonical name.
    if let Some(dir) = packs_dir()
        && let Ok(entries) = std::fs::read_dir(&dir)
    {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("voice_chat-icons")
                && name.ends_with(".zip")
                && let Err(e) = std::fs::remove_file(entry.path())
            {
                eprintln!("voice_chat: removing stale pack {name}: {e}");
            }
        }
    }

    // `add` copies the pack in; `on` turns it on by file name. Both are
    // reported but non-fatal.
    let add = Command::new(&launcher)
        .args(["packs", "add"])
        .arg(&pack)
        .output()
        .map_err(|e| format!("running packs add: {e}"))?;
    if !add.status.success() {
        eprintln!(
            "voice_chat: packs add: {}",
            String::from_utf8_lossy(&add.stderr).trim()
        );
    }
    let on = Command::new(&launcher)
        .args(["packs", "on", PACK])
        .output()
        .map_err(|e| format!("running packs on: {e}"))?;
    if !on.status.success() {
        eprintln!(
            "voice_chat: packs on: {}",
            String::from_utf8_lossy(&on.stderr).trim()
        );
    } else {
        eprintln!("voice_chat: icon resource pack installed and turned on");
    }
    Ok(())
}

/// The launcher's resource packs folder (next to the game data).
fn packs_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
        })?;
    Some(base.join("horizon").join("resourcepacks"))
}

/// `horizon-launcher` from PATH, or the usual install location.
fn find_launcher() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join("horizon-launcher");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let usr = PathBuf::from("/usr/bin/horizon-launcher");
    usr.is_file().then_some(usr)
}
