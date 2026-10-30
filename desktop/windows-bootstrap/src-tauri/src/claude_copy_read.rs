//! ABF137: caller-selected copy only, no discovery or writes.
use abf_collection_boundary::*;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::fictional_file_payload as payload;
#[cfg(any(windows, test))]
#[allow(unsafe_code)]
#[path = "claude_file_dialog_windows.rs"]
mod dialog;
#[cfg(windows)]
#[allow(unsafe_code)]
#[path = "copy_volume_windows.rs"]
mod volume;

// One identity slot per retained outcome, including ineligible observations.
const COPY_MAX_RECORDS: usize = 1024;

pub struct CopyReader {
    root: Option<PathBuf>,
    attempted: AtomicBool,
    active: AtomicBool,
    #[cfg(test)]
    synthetic: bool,
}
impl CopyReader {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            attempted: AtomicBool::new(false),
            active: AtomicBool::new(false),
            #[cfg(test)]
            synthetic: false,
        }
    }
    #[cfg(test)]
    pub fn synthetic_test_root(root: Option<PathBuf>) -> Self {
        Self {
            synthetic: true,
            ..Self::new(root)
        }
    }
    pub fn run(&self, relative: &str) -> Result<String, &'static str> {
        let _active = self.reserve()?;
        if self.attempted.swap(true, Ordering::SeqCst) {
            return Err("COPY_ATTEMPT_CONSUMED");
        }
        let root = self.root.as_ref().ok_or("COPY_NOT_CONFIGURED")?;
        admit_relative(relative)?;
        self.read_at(root, relative, false)
    }

    fn reserve(&self) -> Result<ActiveSelection<'_>, &'static str> {
        self.active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "COPY_BUSY")?;
        let active = ActiveSelection(&self.active);
        if self.attempted.load(Ordering::SeqCst) {
            return Err("COPY_ATTEMPT_CONSUMED");
        }
        Ok(active)
    }

    #[cfg(windows)]
    pub fn pick(&self, owner: usize) -> Result<Option<String>, &'static str> {
        self.select_with(|| dialog::choose(owner))
    }

    // This callback is private native authority, never an IPC path parameter.
    fn select_with(
        &self,
        choose: impl FnOnce() -> Result<Option<PathBuf>, &'static str>,
    ) -> Result<Option<String>, &'static str> {
        let _active = self.reserve()?;
        let selection = choose();
        if matches!(selection, Ok(None)) {
            return Ok(None);
        }
        // A selected file or dialog failure consumes the shared attempt, even on refusal.
        self.attempted.store(true, Ordering::SeqCst);
        let path = selection?.ok_or("COPY_DIALOG_FAILED")?;
        #[cfg(test)]
        let synthetic = self.synthetic;
        #[cfg(not(test))]
        let synthetic = false;
        if !synthetic {
            admit_selected_text(path.to_str().ok_or("COPY_PATH_REFUSED")?)?;
        }
        let root = path.parent().ok_or("COPY_PATH_REFUSED")?;
        let relative = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("COPY_PATH_REFUSED")?;
        admit_relative(relative)?;
        self.read_at(root, relative, true).map(Some)
    }

    fn read_at(&self, root: &Path, relative: &str, selected: bool) -> Result<String, &'static str> {
        #[cfg(test)]
        let synthetic = self.synthetic;
        #[cfg(not(test))]
        let synthetic = false;
        if !synthetic {
            check_root_text(
                root.to_str()
                    .filter(|text| text.is_ascii())
                    .ok_or("COPY_ROOT_REFUSED")?,
                HostFamily::Windows,
            )
            .map_err(|_| "COPY_ROOT_REFUSED")?;
            #[cfg(windows)]
            {
                volume::require_local_fixed(root)?;
                if selected {
                    volume::require_direct_fixed(root)?;
                }
            }
            #[cfg(not(windows))]
            return Err("COPY_PLATFORM_UNSUPPORTED");
        }
        // Check ancestors from the drive downward, refusing detected redirects before descent.
        let mut ancestors: Vec<_> = root.ancestors().collect();
        ancestors.reverse();
        for ancestor in ancestors {
            let metadata = fs::symlink_metadata(ancestor).map_err(|_| "COPY_ROOT_REFUSED")?;
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                return Err("COPY_ROOT_REFUSED");
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("COPY_ROOT_REFUSED");
                }
            }
        }
        if selected {
            // Immediately revalidate the selected target after checking its ancestors.
            // The collector independently rechecks and opens read-only. Path-based checks
            // reduce ordinary races; they do not establish atomic confinement.
            let metadata =
                fs::symlink_metadata(root.join(relative)).map_err(|_| "COPY_PATH_REFUSED")?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err("COPY_PATH_REFUSED");
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("COPY_PATH_REFUSED");
                }
            }
        }
        let root = AuthorizedRoot::new(root).map_err(|_| "COPY_ROOT_REFUSED")?;
        let id = |text| OpaqueId::new(text).expect("fixed internal copy acquisition label");
        // This identifies the reader host only. It says nothing about the copied records' origin.
        let policy = if synthetic {
            HostPolicy::AllowSimulated
        } else {
            HostPolicy::RequireNative
        };
        let scope = CollectionScope::new(
            EnvironmentIdentity::windows(id("abf137-copy-reader")),
            SourceInstance::new(id("abf137-copy"), Harness::ClaudeCode),
            root,
            policy,
        )
        .map_err(|_| "COPY_PLATFORM_UNSUPPORTED")?
        // Copy-only budgets: one small session file, bounded streaming lines and outcomes.
        // These do not change the historical fictional reader or collector defaults.
        .with_limits(CollectionLimits {
            max_file_bytes: 1024 * 1024,
            max_entries: 1,
            max_depth: 4,
        })
        .map_err(|_| "COPY_READ_FAILED")?;
        let batch = collect_claude_jsonl_file(
            &scope,
            &ScanRun::new(id("abf137-manual-attempt"), CaptureTime::NotRecorded),
            relative,
            ClaudeReadLimits {
                max_files: 1,
                max_total_bytes: 1024 * 1024,
                max_line_bytes: 64 * 1024,
                max_records: COPY_MAX_RECORDS,
            },
        )
        .map_err(|_| "COPY_READ_FAILED")?;
        let result =
            payload::copy_replay_payload_json(&batch.exact_replay_summary(COPY_MAX_RECORDS));
        if result.len() > 16384 {
            return Err("COPY_RESPONSE_INVALID");
        }
        Ok(result)
    }
}

struct ActiveSelection<'a>(&'a AtomicBool);
impl Drop for ActiveSelection<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn admit_selected_text(path: &str) -> Result<(), &'static str> {
    if !path.is_ascii()
        || path.len() > 259
        || !path.ends_with(".jsonl")
        || check_root_text(path, HostFamily::Windows).is_err()
        // Preserve AuthorizedRoot's restriction against authorizing a bare drive root.
        || path[3..].split(['/', '\\']).count() < 2
    {
        return Err("COPY_PATH_REFUSED");
    }
    Ok(())
}

fn direct_fixed_device(kind: u32, device: &str) -> bool {
    fixed_local_volume(kind)
        && device
            .strip_prefix(r"\Device\HarddiskVolume")
            .is_some_and(|suffix| {
                !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
            })
}

pub(crate) fn admit_relative(relative: &str) -> Result<(), &'static str> {
    // Reuse the Windows root grammar for reserved names, streams and unsupported syntax.
    // The synthetic drive prefix is lexical only and is never used for filesystem access.
    if !relative.is_ascii()
        || relative.len() > 512
        || relative.contains('\\')
        || !relative.ends_with(".jsonl")
        || relative.split('/').count() > 4
        || check_root_text(&format!("C:/{relative}"), HostFamily::Windows).is_err()
    {
        return Err("COPY_PATH_REFUSED");
    }
    Ok(())
}

pub(crate) fn fixed_local_volume(kind: u32) -> bool {
    kind == 3
}

#[cfg(test)]
#[path = "claude_picker_tests.rs"]
mod picker_tests;
