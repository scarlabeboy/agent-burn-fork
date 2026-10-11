//! Platform ABI modules. `unsafe` is denied crate-wide (`Cargo.toml`
//! `[lints.rust] unsafe_code = "deny"`) and allowed **only** for the single
//! module below. Nothing else in the crate may contain `unsafe`.
//!
//! The module is selected only for the reviewed LP64 ABI: 64-bit pointers,
//! with GNU x32 explicitly excluded (x32 also reports `x86_64`/`linux`/`gnu`,
//! but has 32-bit `long` and pointers and a different syscall numbering).
//! Every other target gets the uninhabited fail-closed backend in `lib.rs`.

#[cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    target_pointer_width = "64",
    not(target_abi = "x32"),
    not(abf_secure_fs_force_unsupported)
))]
#[allow(unsafe_code)]
pub(crate) mod linux_x86_64;
