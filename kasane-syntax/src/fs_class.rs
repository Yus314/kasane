//! Filesystem-class probe for ADR-051 chunk 3d.
//!
//! Some filesystems (notably NFS, CIFS, FUSE, 9P) install inotify watches
//! that never fire on remote-side modifications. On those, the mtime-poll
//! fallback in `SyntaxManager::run_post_sync` is the real reload signal;
//! the watcher is decorative. On local filesystems, the watcher is
//! authoritative and mtime is for cross-validation only.
//!
//! The probe runs once per buffer (cached on `ActiveBuffer`) and is
//! platform-gated to Linux. Other platforms return [`FsClass::Unknown`],
//! which downstream code treats the same as `InotifyTrusted` — i.e. the
//! mtime path emits a divergence warn but does not drive an extra reparse.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FsClass {
    /// Watcher events are reliable. The mtime check, if it fires, is a
    /// real anomaly.
    InotifyTrusted,
    /// Watcher events are unreliable (remote / FUSE). The mtime check
    /// is the primary reload signal; no anomaly to log on
    /// mtime-only fires.
    InotifyBroken,
    /// No probe available on this platform; behave like
    /// `InotifyTrusted` and let the divergence warn surface drift.
    Unknown,
}

#[cfg(target_os = "linux")]
pub(crate) fn probe(path: &std::path::Path) -> FsClass {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;

    let Ok(cpath) = CString::new(path.as_os_str().as_bytes()) else {
        return FsClass::Unknown;
    };
    let mut buf: MaybeUninit<libc::statfs> = MaybeUninit::uninit();
    // SAFETY: cpath is a valid NUL-terminated C string; buf is a properly
    // sized writable buffer for statfs. The kernel either writes a full
    // statfs or returns -1; we only read on success.
    let rc = unsafe { libc::statfs(cpath.as_ptr(), buf.as_mut_ptr()) };
    if rc != 0 {
        return FsClass::Unknown;
    }
    let statfs = unsafe { buf.assume_init() };
    classify(statfs.f_type as u64)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn probe(_path: &std::path::Path) -> FsClass {
    FsClass::Unknown
}

/// Classify a Linux `f_type` magic number.
///
/// Source: `statfs(2)`, Linux `include/uapi/linux/magic.h`. Filesystems
/// without a stable inotify story are flagged broken; everything else
/// (ext4, btrfs, xfs, tmpfs, overlayfs, …) is trusted.
#[cfg(target_os = "linux")]
fn classify(f_type: u64) -> FsClass {
    const NFS_SUPER_MAGIC: u64 = 0x6969;
    const SMB_SUPER_MAGIC: u64 = 0x517B;
    const CIFS_SUPER_MAGIC: u64 = 0xFF534D42;
    const SMB2_SUPER_MAGIC: u64 = 0xFE534D42;
    const FUSE_SUPER_MAGIC: u64 = 0x65735546;
    const AFS_SUPER_MAGIC: u64 = 0x5346414F;
    const CODA_SUPER_MAGIC: u64 = 0x73757245;
    const V9FS_MAGIC: u64 = 0x01021997;

    match f_type {
        NFS_SUPER_MAGIC | SMB_SUPER_MAGIC | CIFS_SUPER_MAGIC | SMB2_SUPER_MAGIC
        | FUSE_SUPER_MAGIC | AFS_SUPER_MAGIC | CODA_SUPER_MAGIC | V9FS_MAGIC => {
            FsClass::InotifyBroken
        }
        _ => FsClass::InotifyTrusted,
    }
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;

    #[test]
    fn classify_known_broken() {
        assert_eq!(classify(0x6969), FsClass::InotifyBroken); // NFS
        assert_eq!(classify(0x65735546), FsClass::InotifyBroken); // FUSE
        assert_eq!(classify(0xFF534D42), FsClass::InotifyBroken); // CIFS
    }

    #[test]
    fn classify_known_trusted() {
        assert_eq!(classify(0xEF53), FsClass::InotifyTrusted); // ext4
        assert_eq!(classify(0x01021994), FsClass::InotifyTrusted); // tmpfs
        assert_eq!(classify(0x9123683E), FsClass::InotifyTrusted); // btrfs
    }

    #[test]
    fn probe_tmpfile_is_trusted_or_unknown() {
        // tempfile uses /tmp which is typically tmpfs (trusted) but may
        // be a different FS in CI. Accept either trusted or unknown — we
        // only assert it doesn't claim broken.
        let dir = tempfile::tempdir().unwrap();
        let class = probe(dir.path());
        assert_ne!(class, FsClass::InotifyBroken);
    }
}
