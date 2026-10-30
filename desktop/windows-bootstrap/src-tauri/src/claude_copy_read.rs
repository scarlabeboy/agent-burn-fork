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

#[cfg(windows)]
#[allow(unsafe_code)]
#[path = "claude_folder_dialog_windows.rs"]
mod folder_dialog;

// One identity slot per retained outcome, including ineligible observations.
const COPY_MAX_RECORDS: usize = 1024;
// Initial folder product safeguards; the shared collector ceilings remain unchanged.
const FOLDER_MAX_FILE_BYTES: u64 = 3 * 1024 * 1024;
const FOLDER_MAX_TOTAL_BYTES: u64 = 24 * 1024 * 1024;
#[cfg(any(windows, test))]
// BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE | BIF_NONEWFOLDERBUTTON.
const FOLDER_DIALOG_FLAGS: u32 = 0x1 | 0x40 | 0x200;

pub struct CopyReader {
    root: Option<PathBuf>,
    attempted: AtomicBool,
    active: AtomicBool,
    discarded: AtomicBool,
    pending: std::sync::Mutex<Option<(PathBuf, PendingClaudeBatch, usize)>>,
    #[cfg(test)]
    synthetic: bool,
}
impl CopyReader {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            attempted: AtomicBool::new(false),
            active: AtomicBool::new(false),
            discarded: AtomicBool::new(false),
            pending: std::sync::Mutex::new(None),
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

    #[cfg(windows)]
    pub fn pick_folder(&self, owner: usize) -> Result<Option<usize>, &'static str> {
        self.select_folder_with(|| folder_dialog::choose(owner))
    }

    fn select_folder_with(
        &self,
        choose: impl FnOnce() -> Result<Option<PathBuf>, &'static str>,
    ) -> Result<Option<usize>, &'static str> {
        let _active = self.reserve()?;
        let selection = choose();
        if matches!(selection, Ok(None)) {
            // Only a genuine cancellation releases a prior Clear reservation.
            self.discarded.store(false, Ordering::SeqCst);
            return Ok(None);
        }
        self.attempted.store(true, Ordering::SeqCst);
        let path = selection?.ok_or("COPY_DIALOG_FAILED")?;
        self.validate_folder(&path)?;
        let root = AuthorizedRoot::new(&path).map_err(|_| "COPY_ROOT_REFUSED")?;
        let id = |s| OpaqueId::new(s).expect("fixed batch label");
        #[cfg(test)]
        let synthetic = self.synthetic;
        #[cfg(not(test))]
        let synthetic = false;
        let scope = CollectionScope::new(
            EnvironmentIdentity::windows(id("abf148b-folder-reader")),
            SourceInstance::new(id("abf148b-folder"), Harness::ClaudeCode),
            root,
            if synthetic {
                HostPolicy::AllowSimulated
            } else {
                HostPolicy::RequireNative
            },
        )
        .map_err(|_| "COPY_PLATFORM_UNSUPPORTED")?
        .with_limits(CollectionLimits {
            max_file_bytes: FOLDER_MAX_FILE_BYTES,
            max_entries: 1024,
            max_depth: 1,
        })
        .map_err(|_| "COPY_READ_FAILED")?;
        let mut names = Vec::new();
        let mut ignored = 0;
        // Bound both unrelated directory entries and candidate storage; never descend.
        for (index, entry) in fs::read_dir(&path)
            .map_err(|_| "COPY_READ_FAILED")?
            .enumerate()
        {
            if index >= 1024 {
                return Err("COPY_FOLDER_LIMIT");
            }
            let entry = entry.map_err(|_| "COPY_READ_FAILED")?;
            let name = entry.file_name();
            let name = name.to_str().ok_or("COPY_PATH_REFUSED")?;
            if !name.ends_with(".jsonl") {
                ignored += 1;
                continue;
            }
            admit_relative(name)?;
            if name.contains(['/', '\\']) {
                return Err("COPY_PATH_REFUSED");
            }
            if !synthetic {
                admit_selected_text(path.join(name).to_str().ok_or("COPY_PATH_REFUSED")?)?;
            }
            names.push(name.to_owned());
            if names.len() > 16 {
                return Err("COPY_FOLDER_LIMIT");
            }
        }
        let refs: Vec<_> = names.iter().map(String::as_str).collect();
        let pending = prepare_claude_jsonl_files(
            &scope,
            &refs,
            ClaudeReadLimits {
                max_files: 16,
                max_total_bytes: FOLDER_MAX_TOTAL_BYTES,
                max_line_bytes: 256 * 1024,
                max_records: 4096,
            },
        )
        .map_err(|_| "COPY_FOLDER_REFUSED")?;
        self.validate_folder(&path)?;
        let count = pending.file_count();
        let mut slot = self.pending.lock().map_err(|_| "COPY_READ_FAILED")?;
        if !self.discarded.load(Ordering::SeqCst) {
            *slot = Some((path, pending, ignored));
        }
        Ok(Some(count))
    }

    fn validate_folder(&self, path: &Path) -> Result<(), &'static str> {
        #[cfg(test)]
        let synthetic = self.synthetic;
        #[cfg(not(test))]
        let synthetic = false;
        if !synthetic {
            let text = path
                .to_str()
                .filter(|s| s.is_ascii() && s.len() <= 259)
                .ok_or("COPY_ROOT_REFUSED")?;
            check_root_text(text, HostFamily::Windows).map_err(|_| "COPY_ROOT_REFUSED")?;
            #[cfg(windows)]
            {
                volume::require_local_fixed(path)?;
                volume::require_direct_fixed(path)?;
            }
            #[cfg(not(windows))]
            return Err("COPY_PLATFORM_UNSUPPORTED");
        }
        let mut ancestors: Vec<_> = path.ancestors().collect();
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
        Ok(())
    }

    pub fn analyze_folder(&self) -> Result<String, &'static str> {
        self.active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "COPY_BUSY")?;
        let _active = ActiveSelection(&self.active);
        self.attempted.store(true, Ordering::SeqCst);
        if self.discarded.load(Ordering::SeqCst) {
            return Err("COPY_ATTEMPT_CONSUMED");
        }
        let (path, pending, ignored) = self
            .pending
            .lock()
            .map_err(|_| "COPY_READ_FAILED")?
            .take()
            .ok_or("COPY_ATTEMPT_CONSUMED")?;
        self.validate_folder(&path)?;
        let id = OpaqueId::new("abf148b-manual-attempt").expect("fixed label");
        let mut batch = pending
            .collect(&ScanRun::new(id, CaptureTime::NotRecorded))
            .map_err(|_| "COPY_READ_FAILED")?;
        self.validate_folder(&path)?;
        // Ignored entries were not inspected; conservatively report unknown coverage.
        for _ in 0..ignored {
            batch.skipped.push(SkippedEntry::unselected());
        }
        if ignored > 0 {
            batch.completeness = Completeness::Incomplete;
        }
        // One evaluation across every file; never add independent replay summaries.
        let result = payload::copy_replay_payload_json(&batch.exact_replay_summary(4096))
            .replace("configured-windows-copy", "selected-windows-folder");
        let result = crate::claude_copy_activity::folder_payload_json(&batch, result);
        if result.len() > 16384 {
            return Err("COPY_RESPONSE_INVALID");
        }
        Ok(result)
    }

    pub fn clear_folder(&self) {
        self.discarded.store(true, Ordering::SeqCst);
        if let Ok(mut pending) = self.pending.lock() {
            *pending = None;
        }
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
