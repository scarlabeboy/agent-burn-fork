//! Narrow Windows locality probe. No handles, processes, writes or discovery.
use std::path::Path;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetDriveTypeW(root: *const u16) -> u32;
    fn QueryDosDeviceW(name: *const u16, target: *mut u16, capacity: u32) -> u32;
}

pub(super) fn require_direct_fixed(root: &Path) -> Result<(), &'static str> {
    // Only the selected drive is queried. Refuse DOS aliases (including SUBST)
    // and uncertain device mappings before touching any source directory.
    let text = root.to_str().ok_or("COPY_PATH_REFUSED")?;
    let name = [u16::from(text.as_bytes()[0]), u16::from(b':'), 0];
    let drive = [name[0], name[1], u16::from(b'\\'), 0];
    let mut target = [0_u16; 1024];
    // SAFETY: all buffers are live and bounded for the synchronous Kernel32 calls.
    let (kind, count) = unsafe {
        (
            GetDriveTypeW(drive.as_ptr()),
            QueryDosDeviceW(name.as_ptr(), target.as_mut_ptr(), 1024),
        )
    };
    if count == 0 || count as usize > target.len() {
        return Err("COPY_LOCALITY_UNCOVERED");
    }
    let end = target[..count as usize]
        .iter()
        .position(|unit| *unit == 0)
        .ok_or("COPY_LOCALITY_UNCOVERED")?;
    let device = String::from_utf16(&target[..end]).map_err(|_| "COPY_LOCALITY_UNCOVERED")?;
    if super::direct_fixed_device(kind, &device) {
        Ok(())
    } else {
        Err("COPY_LOCALITY_UNCOVERED")
    }
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
