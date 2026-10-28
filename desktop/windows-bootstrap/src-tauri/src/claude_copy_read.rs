//! ABF137: caller-selected copy only, no discovery or writes.
use abf_collection_boundary::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::fictional_file_payload as payload;
#[cfg(windows)]
#[allow(unsafe_code)]
#[path = "copy_volume_windows.rs"]
mod volume;

pub struct CopyReader {
    root: Option<PathBuf>,
    attempted: AtomicBool,
    #[cfg(test)]
    synthetic: bool,
}
impl CopyReader {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            attempted: AtomicBool::new(false),
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
        if self.attempted.swap(true, Ordering::SeqCst) {
            return Err("COPY_ATTEMPT_CONSUMED");
        }
        let root = self.root.as_ref().ok_or("COPY_NOT_CONFIGURED")?;
        admit_relative(relative)?;
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
            volume::require_local_fixed(root)?;
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
                max_records: 1024,
            },
        )
        .map_err(|_| "COPY_READ_FAILED")?;
        Ok(payload::copy_payload_json(&batch.summary()))
    }
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
