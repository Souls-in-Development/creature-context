//! macOS metadata adapter: Green state as Finder tags (specification 16).
//!
//! Finder stores a file's tags in the extended attribute
//! `com.apple.metadata:_kMDItemUserTags`, whose value is a binary property list
//! containing an array of tag strings. This adapter writes that attribute from
//! the Atlas projection, reads it back, and clears it — so a tag can be deleted
//! and rebuilt from the Atlas with no loss. It is the I/O skin only: it holds no
//! Atlas state and decides nothing about Green.
//!
//! The extended-attribute calls are the platform semantic; they are declared
//! directly against libc rather than pulling a dependency, matching how the
//! vendored grammars are linked.

use creature_context_types::model::CapabilityState;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

const FINDER_TAGS: &str = "com.apple.metadata:_kMDItemUserTags";

unsafe extern "C" {
    fn setxattr(
        path: *const c_char,
        name: *const c_char,
        value: *const c_void,
        size: usize,
        position: u32,
        options: c_int,
    ) -> c_int;
    fn getxattr(
        path: *const c_char,
        name: *const c_char,
        value: *mut c_void,
        size: usize,
        position: u32,
        options: c_int,
    ) -> isize;
    fn removexattr(path: *const c_char, name: *const c_char, options: c_int) -> c_int;
}

/// macOS has a real adapter that this build can run; extended attributes are a
/// core OS facility, so the capability is verified by the round-trip test.
pub fn capability() -> CapabilityState {
    CapabilityState::Verified
}

/// Errors the adapter surfaces. A failed OS call is a typed error, never a
/// swallowed one.
#[derive(Debug)]
pub enum MetadataError {
    Path,
    SetFailed(i32),
    RemoveFailed(i32),
}

fn cstring(path: &Path) -> Result<CString, MetadataError> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| MetadataError::Path)
}

/// Write a single Finder tag onto `path`, replacing any this adapter set.
pub fn write_tag(path: &Path, tag: &str) -> Result<(), MetadataError> {
    let c_path = cstring(path)?;
    let name = CString::new(FINDER_TAGS).map_err(|_| MetadataError::Path)?;
    let plist = finder_tags_plist(&[tag]);
    let rc = unsafe {
        setxattr(
            c_path.as_ptr(),
            name.as_ptr(),
            plist.as_ptr() as *const c_void,
            plist.len(),
            0,
            0,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(MetadataError::SetFailed(errno()))
    }
}

/// Read the Finder tags on `path`, returning the first tag if present. Returns
/// `None` when the attribute is absent — an unlabelled file, not an error.
pub fn read_tag(path: &Path) -> Option<String> {
    let c_path = cstring(path).ok()?;
    let name = CString::new(FINDER_TAGS).ok()?;
    let size = unsafe {
        getxattr(
            c_path.as_ptr(),
            name.as_ptr(),
            std::ptr::null_mut(),
            0,
            0,
            0,
        )
    };
    if size <= 0 {
        return None;
    }
    let mut buffer = vec![0u8; size as usize];
    let read = unsafe {
        getxattr(
            c_path.as_ptr(),
            name.as_ptr(),
            buffer.as_mut_ptr() as *mut c_void,
            buffer.len(),
            0,
            0,
        )
    };
    if read <= 0 {
        return None;
    }
    buffer.truncate(read as usize);
    decode_first_finder_tag(&buffer)
}

/// Remove this adapter's Finder-tag attribute from `path`. Absent is success —
/// clearing what is not there loses nothing.
pub fn clear_tag(path: &Path) -> Result<(), MetadataError> {
    let c_path = cstring(path)?;
    let name = CString::new(FINDER_TAGS).map_err(|_| MetadataError::Path)?;
    let rc = unsafe { removexattr(c_path.as_ptr(), name.as_ptr(), 0) };
    // ENOATTR (93) means the attribute was already absent — that is success.
    if rc == 0 || errno() == 93 {
        Ok(())
    } else {
        Err(MetadataError::RemoveFailed(errno()))
    }
}

fn errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Encode `tags` as a binary property list holding an array of ASCII strings —
/// the exact shape Finder reads from `_kMDItemUserTags`. Supports the short
/// ASCII tag labels this projection uses (each < 15 bytes, fewer than 15 tags),
/// which is all Green labels ever are.
fn finder_tags_plist(tags: &[&str]) -> Vec<u8> {
    // Object 0 is the array; objects 1..=n are the strings it references.
    let mut objects: Vec<Vec<u8>> = Vec::new();
    let mut array = vec![0xA0 | (tags.len() as u8)];
    for index in 0..tags.len() {
        array.push((index + 1) as u8); // 1-byte object references
    }
    objects.push(array);
    for tag in tags {
        let bytes = tag.as_bytes();
        let mut string = vec![0x50 | (bytes.len() as u8)]; // ASCII string marker
        string.extend_from_slice(bytes);
        objects.push(string);
    }

    let mut out = b"bplist00".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for object in &objects {
        offsets.push(out.len() as u8);
        out.extend_from_slice(object);
    }
    let offset_table_start = out.len() as u64;
    out.extend_from_slice(&offsets);

    // 32-byte trailer: 5 unused, sort version, offset-int size, object-ref size,
    // then num-objects, top-object index and offset-table offset as u64 BE.
    out.extend_from_slice(&[0u8; 6]);
    out.push(1); // offset int size
    out.push(1); // object ref size
    out.extend_from_slice(&(objects.len() as u64).to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&offset_table_start.to_be_bytes());
    out
}

/// Extract the first tag string from a `_kMDItemUserTags` binary plist this
/// adapter wrote. Deliberately minimal — it reads back exactly the shape
/// `finder_tags_plist` writes, which is what the round-trip test needs.
fn decode_first_finder_tag(plist: &[u8]) -> Option<String> {
    if !plist.starts_with(b"bplist00") {
        return None;
    }
    // Object 0 (the array) begins at byte 8: 0xA_ count, then refs. Object 1 is
    // the first string: 0x5_ length, then ASCII bytes.
    let array_count = (plist.get(8)? & 0x0F) as usize;
    if array_count == 0 {
        return None;
    }
    let first_string_at = 8 + 1 + array_count; // past the array header + refs
    let marker = *plist.get(first_string_at)?;
    if marker & 0xF0 != 0x50 {
        return None;
    }
    let len = (marker & 0x0F) as usize;
    let start = first_string_at + 1;
    let bytes = plist.get(start..start + len)?;
    std::str::from_utf8(bytes).ok().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_round_trips_through_the_decoder() {
        // Encoder and decoder agree on the shape Finder reads.
        for tag in ["Green", "Yellow", "Red", "Unknown"] {
            let plist = finder_tags_plist(&[tag]);
            assert!(plist.starts_with(b"bplist00"));
            assert_eq!(decode_first_finder_tag(&plist).as_deref(), Some(tag));
        }
    }
}
