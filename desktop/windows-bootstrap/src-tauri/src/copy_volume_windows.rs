//! Narrow Windows locality probe. No handles, processes, writes or discovery.
use std::path::Path;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetDriveTypeW(root: *const u16) -> u32;
}
pub(super) fn require_local_fixed(root: &Path) -> Result<(), &'static str> {
    // The caller checked ordinary absolute drive syntax. Check the drive before
    // touching the configured directory; remote, removable and unknown are refused.
    let text = root.to_str().ok_or("COPY_ROOT_REFUSED")?;
    // Normalize the separator for the API even when root syntax uses forward slashes.
    let drive = [
        u16::from(text.as_bytes()[0]),
        u16::from(b':'),
        u16::from(b'\\'),
        0,
    ];
    // SAFETY: drive is a live NUL-terminated UTF-16 drive-root string. The API
    // reads it synchronously, retains no pointer and has no output buffer.
    let kind = unsafe { GetDriveTypeW(drive.as_ptr()) };
    if super::fixed_local_volume(kind) {
        Ok(())
    } else {
        Err("COPY_LOCALITY_UNCOVERED")
    }
}
