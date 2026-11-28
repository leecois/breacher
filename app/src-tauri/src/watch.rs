//! Watching the game for a breach to open, for a player who wants the route without pressing anything.

use breach_core::image::apart;
use breach_core::reader::Reading;
use std::time::Duration;

/// Time between two looks at the game while no breach is being read.
pub const TICK: Duration = Duration::from_millis(600);
/// Time before the second look that has to agree with the first.
pub const AGAIN: Duration = Duration::from_millis(200);
/// Time between two searches for a game window that is not there.
pub const SEEK: Duration = Duration::from_secs(3);
/// The size a frame is shrunk to for telling whether the picture changed.
pub const GLANCE: (usize, usize) = (48, 27);
/// Mean difference per value, out of 255, over which the picture is no longer the one last read; an open breach stays under 2.
const CHANGED: f32 = 4.0;
/// Looks after which a picture is read whatever it looks like, in case a change was missed.
const ANYWAY: u32 = 8;

/// What a read made of the screen, in a form two reads can be compared by.
pub type Made = (usize, Vec<u8>, Vec<Vec<u8>>, usize);

pub fn made(reading: &Reading) -> Made {
    (reading.n, reading.matrix.clone(), reading.sequences.clone(), reading.buffer)
}

/// What the watch remembers between looks.
#[derive(Default)]
pub struct Watch {
    /// The picture the reader was last tried on.
    tried: Option<Vec<u8>>,
    since: u32,
    /// What the last read made of the screen, until a second read agrees: a screen still opening is not taken at its word.
    unconfirmed: Option<Made>,
    /// The breach whose route is drawn: it is not drawn again for as long as it stays on screen.
    shown: Option<Made>,
}

impl Watch {
    /// Whether a first read is waiting for the second.
    pub fn pending(&self) -> bool {
        self.unconfirmed.is_some()
    }

    /// Whether the picture is worth a read: it changed since the last one, a first read waits for its second, or it has been a while.
    pub fn wants(&mut self, glance: Vec<u8>) -> bool {
        self.since += 1;
        let fresh = self.tried.as_ref().is_none_or(|tried| apart(tried, &glance) > CHANGED);
        if !(fresh || self.pending() || self.since >= ANYWAY) {
            return false;
        }
        self.tried = Some(glance);
        self.since = 0;
        true
    }

    /// Takes in what a read made of the screen; true when its route is to be drawn: two reads in a row agree on a breach that is not the one already shown.
    pub fn agrees(&mut self, made: Option<Made>) -> bool {
        if made != self.shown {
            self.shown = None;
        }
        if made.is_none() || made == self.shown {
            self.unconfirmed = None;
            return false;
        }
        if made != self.unconfirmed {
            self.unconfirmed = made;
            return false;
        }
        self.unconfirmed = None;
        self.shown = made;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn breach(code: u8) -> Option<Made> {
        Some((2, vec![code, 1, 2, 3], vec![vec![code, 1]], 4))
    }

    #[test]
    fn a_breach_is_drawn_once_two_reads_agree() {
        let mut watch = Watch::default();
        assert!(!watch.agrees(breach(0)) && watch.pending());
        assert!(watch.agrees(breach(0)) && !watch.pending());
        // while it stays on screen it is not drawn again
        assert!(!watch.agrees(breach(0)) && !watch.pending());
        assert!(!watch.agrees(breach(0)));
    }

    #[test]
    fn a_screen_still_opening_is_not_taken_at_its_word() {
        let mut watch = Watch::default();
        assert!(!watch.agrees(breach(0)));
        // the second read differs: it becomes the one to confirm
        assert!(!watch.agrees(breach(5)) && watch.pending());
        assert!(!watch.agrees(None) && !watch.pending());
        assert!(!watch.agrees(breach(5)));
        assert!(watch.agrees(breach(5)));
    }

    #[test]
    fn the_same_breach_is_drawn_again_after_it_has_been_away() {
        let mut watch = Watch::default();
        watch.agrees(breach(0));
        assert!(watch.agrees(breach(0)));
        assert!(!watch.agrees(None));
        assert!(!watch.agrees(breach(0)));
        assert!(watch.agrees(breach(0)));
        // and another breach right after it is drawn too
        assert!(!watch.agrees(breach(3)));
        assert!(watch.agrees(breach(3)));
    }

    #[test]
    fn a_picture_is_read_when_it_changes_and_now_and_then_when_it_does_not() {
        let mut watch = Watch::default();
        let (still, moved) = (vec![10u8; 16], vec![60u8; 16]);
        assert!(watch.wants(still.clone()));
        for _ in 0..ANYWAY - 1 {
            assert!(!watch.wants(still.clone()));
        }
        assert!(watch.wants(still.clone()));
        assert!(!watch.wants(still.clone()));
        assert!(watch.wants(moved.clone()));
        assert!(!watch.wants(moved.clone()));
        // a first read that waits for its second gets it whatever the picture
        watch.agrees(breach(0));
        assert!(watch.wants(moved));
    }
}
