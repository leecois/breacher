//! Where a window stands among the others: which is on top, and which keeps itself there.

#[cfg(windows)]
mod sys {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST, SWP_ASYNCWINDOWPOS,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WS_EX_TOPMOST,
    };

    pub fn raise(window: isize) -> bool {
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS;
        // SAFETY: the call takes a window handle by value and fails cleanly on one that is gone
        unsafe { SetWindowPos(HWND(window as _), Some(HWND_TOPMOST), 0, 0, 0, 0, flags).is_ok() }
    }

    pub fn topmost(window: isize) -> Option<bool> {
        // SAFETY: as above; zero is what a window that is gone answers
        let style = unsafe { GetWindowLongW(HWND(window as _), GWL_EXSTYLE) };
        (style != 0).then_some(style as u32 & WS_EX_TOPMOST.0 != 0)
    }
}

/// Puts a window above every other, including those that keep themselves on top, without giving it the keyboard.
pub fn raise(window: isize) -> bool {
    #[cfg(windows)]
    {
        sys::raise(window)
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        false
    }
}

/// Whether a window keeps itself above ordinary windows, where the system tells.
pub fn topmost(window: isize) -> Option<bool> {
    #[cfg(windows)]
    {
        sys::topmost(window)
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_that_does_not_exist_is_neither_raised_nor_known() {
        assert!(!raise(0));
        assert_eq!(topmost(0), None);
    }
}
