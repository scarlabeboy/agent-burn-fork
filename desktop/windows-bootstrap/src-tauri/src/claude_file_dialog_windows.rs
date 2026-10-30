//! Single native selection. No frontend path, hooks, enumeration or ABF-controlled path persistence.
use std::{ffi::c_void, path::PathBuf};

// Windows SDK OPENFILENAMEW layout, restricted to the Windows x64 host.
#[repr(C)]
#[allow(non_snake_case)]
struct OpenFileNameW {
    lStructSize: u32,
    hwndOwner: *mut c_void,
    hInstance: *mut c_void,
    lpstrFilter: *const u16,
    lpstrCustomFilter: *mut u16,
    nMaxCustFilter: u32,
    nFilterIndex: u32,
    lpstrFile: *mut u16,
    nMaxFile: u32,
    lpstrFileTitle: *mut u16,
    nMaxFileTitle: u32,
    lpstrInitialDir: *const u16,
    lpstrTitle: *const u16,
    Flags: u32,
    nFileOffset: u16,
    nFileExtension: u16,
    lpstrDefExt: *const u16,
    lCustData: isize,
    lpfnHook: Option<unsafe extern "system" fn(*mut c_void, u32, usize, isize) -> usize>,
    lpTemplateName: *const u16,
    pvReserved: *mut c_void,
    dwReserved: u32,
    FlagsEx: u32,
}

#[cfg(windows)]
#[link(name = "comdlg32")]
unsafe extern "system" {
    fn GetOpenFileNameW(request: *mut OpenFileNameW) -> i32;
    fn CommDlgExtendedError() -> u32;
}

#[cfg(windows)]
pub(super) fn choose(owner: usize) -> Result<Option<PathBuf>, &'static str> {
    choose_with(owner, |request| {
        // SAFETY: owner comes from the live Tauri window. All input strings and the
        // bounded writable buffer live through this modal synchronous call. No hook runs.
        if unsafe { GetOpenFileNameW(request) } != 0 {
            return Ok(true);
        }
        // SAFETY: same thread, immediately following the failed dialog.
        if unsafe { CommDlgExtendedError() } == 0 {
            Ok(false)
        } else {
            Err("COPY_DIALOG_FAILED")
        }
    })
}

fn choose_with(
    owner: usize,
    show: impl FnOnce(&mut OpenFileNameW) -> Result<bool, &'static str>,
) -> Result<Option<PathBuf>, &'static str> {
    const OFN_HIDEREADONLY: u32 = 0x4;
    const OFN_NOCHANGEDIR: u32 = 0x8;
    const OFN_PATHMUSTEXIST: u32 = 0x800;
    const OFN_FILEMUSTEXIST: u32 = 0x1000;
    const OFN_NONETWORKBUTTON: u32 = 0x20000;
    const OFN_EXPLORER: u32 = 0x80000;
    const OFN_NODEREFERENCELINKS: u32 = 0x100000;
    const OFN_DONTADDTORECENT: u32 = 0x2000000;
    let filter: Vec<u16> = "Claude JSONL file (*.jsonl)\0*.jsonl\0\0"
        .encode_utf16()
        .collect();
    let title: Vec<u16> = "Choose JSONL file\0".encode_utf16().collect();
    let mut selected = [0_u16; 260];
    // SAFETY: this C structure contains only integers, raw pointers and an optional
    // function pointer; zero is valid for every field. Required fields follow.
    let mut request: OpenFileNameW = unsafe { std::mem::zeroed() };
    request.lStructSize = std::mem::size_of::<OpenFileNameW>() as u32;
    request.hwndOwner = owner as *mut c_void;
    request.lpstrFilter = filter.as_ptr();
    request.nFilterIndex = 1;
    request.lpstrFile = selected.as_mut_ptr();
    request.nMaxFile = selected.len() as u32;
    request.lpstrTitle = title.as_ptr();
    request.Flags = OFN_HIDEREADONLY
        | OFN_NOCHANGEDIR
        | OFN_PATHMUSTEXIST
        | OFN_FILEMUSTEXIST
        | OFN_NONETWORKBUTTON
        | OFN_EXPLORER
        | OFN_NODEREFERENCELINKS
        | OFN_DONTADDTORECENT;
    // No multiselect, custom hooks, default source directory or ABF-controlled path persistence.
    if !show(&mut request)? {
        return Ok(None);
    }
    let end = selected
        .iter()
        .position(|unit| *unit == 0)
        .ok_or("COPY_PATH_REFUSED")?;
    if end == 0 {
        return Err("COPY_PATH_REFUSED");
    }
    let path = String::from_utf16(&selected[..end]).map_err(|_| "COPY_PATH_REFUSED")?;
    Ok(Some(PathBuf::from(path)))
}

// ABI assertions are compile-time checks, not native execution evidence.
const _: () = assert!(std::mem::size_of::<OpenFileNameW>() == 152);
const _: () = assert!(std::mem::offset_of!(OpenFileNameW, Flags) == 96);
const _: () = assert!(std::mem::offset_of!(OpenFileNameW, FlagsEx) == 148);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_policy_is_single_file_without_hooks_or_retained_source_directory() {
        assert_eq!(
            choose_with(42, |request| {
                assert_eq!(request.hwndOwner as usize, 42);
                assert_eq!(request.nMaxFile, 260);
                assert_eq!(request.Flags & 0x200, 0); // OFN_ALLOWMULTISELECT absent
                assert_eq!(request.Flags & 0x20, 0); // OFN_ENABLEHOOK absent
                assert_ne!(request.Flags & 0x8, 0); // OFN_NOCHANGEDIR
                assert_ne!(request.Flags & 0x100000, 0); // OFN_NODEREFERENCELINKS
                assert_ne!(request.Flags & 0x2000000, 0); // OFN_DONTADDTORECENT
                assert!(request.lpstrInitialDir.is_null());
                assert!(request.lpfnHook.is_none());
                Ok(false)
            }),
            Ok(None)
        );
    }

    #[test]
    fn dialog_failure_is_distinct_from_cancellation() {
        assert_eq!(
            choose_with(42, |_| Err("COPY_DIALOG_FAILED")),
            Err("COPY_DIALOG_FAILED")
        );
    }

    #[test]
    fn selected_native_buffer_decodes_only_inside_rust() {
        assert_eq!(
            choose_with(42, |request| {
                let path: Vec<u16> = "C:\\synthetic\\session.jsonl\0".encode_utf16().collect();
                // SAFETY: synthetic dialog output fits the live 260-unit buffer.
                unsafe {
                    std::ptr::copy_nonoverlapping(path.as_ptr(), request.lpstrFile, path.len());
                }
                Ok(true)
            }),
            Ok(Some(PathBuf::from(r"C:\synthetic\session.jsonl")))
        );
    }

    #[test]
    fn empty_or_unterminated_success_buffers_are_refused() {
        assert_eq!(choose_with(42, |_| Ok(true)), Err("COPY_PATH_REFUSED"));
        assert_eq!(
            choose_with(42, |request| {
                // SAFETY: filling exactly the synthetic output buffer capacity.
                unsafe {
                    std::slice::from_raw_parts_mut(request.lpstrFile, 260).fill(65);
                }
                Ok(true)
            }),
            Err("COPY_PATH_REFUSED")
        );
    }
}
