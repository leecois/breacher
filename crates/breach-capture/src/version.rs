//! The version of the program behind a window, read from its executable.

/// The two version numbers an executable carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub product: [u16; 4],
    pub file: [u16; 4],
}

fn dotted(v: [u16; 4]) -> String {
    // trailing zeros say nothing, but a version keeps at least two numbers
    let used = v.iter().rposition(|n| *n != 0).map_or(2, |last| (last + 1).max(2));
    v[..used].iter().map(u16::to_string).collect::<Vec<_>>().join(".")
}

impl Version {
    /// The version as a player would quote it, with the build behind it when that is another number.
    pub fn label(&self) -> String {
        if self.product == self.file || self.product == [0; 4] {
            dotted(self.file)
        } else if self.file == [0; 4] {
            dotted(self.product)
        } else {
            format!("{} (build {})", dotted(self.product), dotted(self.file))
        }
    }
}

#[cfg(windows)]
mod sys {
    use super::Version;
    use windows::core::{w, PCWSTR, PWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    pub fn of_process(pid: u32) -> Option<Version> {
        let mut path = [0u16; 1024];
        let mut len = path.len() as u32;
        // SAFETY: the buffer outlives the call and `len` carries its capacity
        let named = unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let named = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(path.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(process);
            named
        };
        named.ok()?;
        // the terminator sits right after the `len` characters that were written
        of_wide(path.get(..=len as usize)?)
    }

    #[cfg(test)]
    pub fn of_file(path: &std::path::Path) -> Option<Version> {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        of_wide(&wide)
    }

    fn of_wide(path: &[u16]) -> Option<Version> {
        if path.last() != Some(&0) {
            return None;
        }
        let name = PCWSTR(path.as_ptr());
        // SAFETY: `name` is terminated, and the block is as large as the system asked for and outlives the pointer into it
        unsafe {
            let size = GetFileVersionInfoSizeW(name, None);
            if size == 0 {
                return None;
            }
            let mut block = vec![0u8; size as usize];
            GetFileVersionInfoW(name, None, size, block.as_mut_ptr().cast()).ok()?;
            let mut at = std::ptr::null_mut();
            let mut len = 0u32;
            let found = VerQueryValueW(block.as_ptr().cast(), w!("\\"), &mut at, &mut len);
            if !found.as_bool() || at.is_null() || (len as usize) < size_of::<VS_FIXEDFILEINFO>() {
                return None;
            }
            let info = std::ptr::read_unaligned(at as *const VS_FIXEDFILEINFO);
            let split = |high: u32, low: u32| {
                [(high >> 16) as u16, high as u16, (low >> 16) as u16, low as u16]
            };
            Some(Version {
                product: split(info.dwProductVersionMS, info.dwProductVersionLS),
                file: split(info.dwFileVersionMS, info.dwFileVersionLS),
            })
        }
    }
}

/// The version of the program that owns a process, where the system records one.
pub fn of_process(pid: u32) -> Option<Version> {
    #[cfg(windows)]
    {
        sys::of_process(pid)
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_reads_the_way_a_player_quotes_it() {
        let patched = Version { product: [2, 31, 0, 0], file: [3, 0, 80, 34703] };
        assert_eq!(patched.label(), "2.31 (build 3.0.80.34703)");
        let same = Version { product: [1, 4, 2, 0], file: [1, 4, 2, 0] };
        assert_eq!(same.label(), "1.4.2");
        let build_only = Version { product: [0; 4], file: [10, 0, 0, 0] };
        assert_eq!(build_only.label(), "10.0");
        let product_only = Version { product: [2, 0, 0, 0], file: [0; 4] };
        assert_eq!(product_only.label(), "2.0");
    }

    #[test]
    fn a_process_without_a_recorded_version_has_none() {
        assert_eq!(of_process(u32::MAX), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_system_program_reports_its_version() {
        let root = std::env::var_os("SystemRoot").expect("SystemRoot");
        let shell = std::path::Path::new(&root).join("System32").join("cmd.exe");
        let v = sys::of_file(&shell).expect("the command shell carries a version");
        assert!(v.file[0] >= 6, "{v:?}");
        println!("cmd.exe: {}", v.label());
    }
}
