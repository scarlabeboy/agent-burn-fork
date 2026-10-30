//! Native shell directory dialog. Only Rust receives the selected filesystem path.
use std::{ffi::c_void, path::PathBuf};
#[repr(C)]
struct BrowseInfo {
    owner: *mut c_void,
    root: *const c_void,
    display: *mut u16,
    title: *const u16,
    flags: u32,
    callback: Option<unsafe extern "system" fn(*mut c_void, u32, isize, isize) -> i32>,
    parameter: isize,
    image: i32,
}
#[link(name = "shell32")]
unsafe extern "system" {
    fn SHBrowseForFolderW(info: *const BrowseInfo) -> *mut c_void;
    fn SHGetPathFromIDListW(item: *const c_void, path: *mut u16) -> i32;
}
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, flags: u32) -> i32;
    fn CoUninitialize();
    fn CoTaskMemFree(memory: *mut c_void);
}
struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: balances successful initialization on this synchronous thread.
        unsafe { CoUninitialize() };
    }
}
struct Item(*mut c_void);
impl Drop for Item {
    fn drop(&mut self) {
        // SAFETY: shell-allocated PIDL, freed once with the documented allocator.
        unsafe { CoTaskMemFree(self.0) };
    }
}
pub(super) fn choose(owner: usize) -> Result<Option<PathBuf>, &'static str> {
    // SAFETY: no reserved pointer; require an STA for the modal shell dialog.
    if unsafe { CoInitializeEx(std::ptr::null_mut(), 2) } < 0 {
        return Err("COPY_DIALOG_FAILED");
    }
    let _apartment = Apartment;
    let title: Vec<_> = "Choose Claude JSONL folder\0".encode_utf16().collect();
    let mut display = [0u16; 260];
    let info = BrowseInfo {
        owner: owner as *mut c_void,
        root: std::ptr::null(),
        display: display.as_mut_ptr(),
        title: title.as_ptr(),
        flags: super::FOLDER_DIALOG_FLAGS,
        callback: None,
        parameter: 0,
        image: 0,
    };
    // SAFETY: bounded live buffers and live owner, no callback. Synchronous modal call.
    let selected = unsafe { SHBrowseForFolderW(&info) };
    if selected.is_null() {
        return Ok(None);
    }
    let item = Item(selected);
    let mut path = [0u16; 260];
    // SAFETY: valid selected PIDL, MAX_PATH writable buffer as required by the API.
    if unsafe { SHGetPathFromIDListW(item.0, path.as_mut_ptr()) } == 0 {
        return Err("COPY_PATH_REFUSED");
    }
    let end = path
        .iter()
        .position(|u| *u == 0)
        .filter(|end| *end > 0)
        .ok_or("COPY_PATH_REFUSED")?;
    let path = String::from_utf16(&path[..end]).map_err(|_| "COPY_PATH_REFUSED")?;
    Ok(Some(PathBuf::from(path)))
}
const _: () = assert!(std::mem::size_of::<BrowseInfo>() == 64);
