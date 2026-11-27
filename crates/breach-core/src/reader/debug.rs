use crate::image::Plane;

pub(super) fn debug_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("BREACH_DEBUG").is_some())
}

macro_rules! dbg_log {
    ($($arg:tt)*) => {
        if $crate::reader::debug::debug_on() {
            eprintln!($($arg)*);
        }
    };
}

pub(super) use dbg_log;

/// Debug aid: with BREACH_DEBUG_DIR set, intermediate planes are written there as PGM images.
pub(super) fn dump_plane(name: &str, p: &Plane, scale: u8) {
    if let Some(dir) = std::env::var_os("BREACH_DEBUG_DIR") {
        // keep the first dump of each name: that is the first scale attempt, the one that matters
        if std::path::Path::new(&dir).join(format!("{name}.pgm")).exists() {
            return;
        }
        let mut bytes = format!("P5\n{} {}\n255\n", p.w, p.h).into_bytes();
        bytes.extend(p.data.iter().map(|&v| v.saturating_mul(scale)));
        let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}.pgm")), bytes);
    }
}
