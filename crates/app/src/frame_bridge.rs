//! The raw-pixel path to Qt Quick: `cpp/picture_provider.cpp` serves pictures the Rust core drew
//! (the canvas, the video preview, timeline thumbnails) as `image://pictures/<id>`. This is the
//! only place the editor calls into C++ by hand.

use std::collections::{HashMap, VecDeque};
use std::ffi::CString;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use cxx_qt_lib::QQmlApplicationEngine;

unsafe extern "C" {
    fn od_picture_publish(id: *const std::ffi::c_char, pixels: *const u8, w: i32, h: i32) -> bool;
    fn od_picture_forget(id: *const std::ffi::c_char);
    fn od_picture_forget_prefix(prefix: *const std::ffi::c_char);
    fn od_pictures_install(engine: *mut QQmlApplicationEngine);
}

/// Pictures of one kind (one id prefix) that stay published: the newest few, which an image item
/// may still be asking for while the next ones arrive. Sixteen is enough for a picture that is
/// loaded a few frames late and small enough to forget (a canvas picture is about a megabyte).
const KEPT: usize = 16;

static SERIAL: AtomicU64 = AtomicU64::new(1);
static RECENT: Mutex<Option<HashMap<String, VecDeque<String>>>> = Mutex::new(None);

/// Makes `image://pictures/<id>` serve the pictures [`publish`] hands over.
pub fn install(engine: core::pin::Pin<&mut QQmlApplicationEngine>) {
    // SAFETY: the engine is alive for the call and `od_pictures_install` only registers a
    // provider on it; the pointer is not kept. The pin is not moved out of.
    unsafe { od_pictures_install(engine.get_unchecked_mut()) }
}

fn publish_as(id: &str, width: u32, height: u32, pixels: &[u8]) -> bool {
    let (Ok(w), Ok(h), Ok(name)) = (
        i32::try_from(width),
        i32::try_from(height),
        CString::new(id),
    ) else {
        return false;
    };
    if pixels.len() != width as usize * height as usize * 4 {
        return false;
    }
    // SAFETY: `name` is NUL-terminated and `pixels` holds exactly `width * height * 4` readable
    // bytes (checked above); the callee copies both before returning.
    unsafe { od_picture_publish(name.as_ptr(), pixels.as_ptr(), w, h) }
}

/// Hands Qt a picture of premultiplied RGBA bytes under a fresh id starting with `prefix` and
/// returns its `image://` address. Older pictures of the same prefix are dropped past [`KEPT`].
/// `None` when the bytes do not fit the size or Qt cannot hold an image that large.
pub fn publish(prefix: &str, width: u32, height: u32, pixels: &[u8]) -> Option<String> {
    let id = format!("{prefix}{}", SERIAL.fetch_add(1, Ordering::Relaxed));
    if !publish_as(&id, width, height, pixels) {
        return None;
    }
    let mut guard = RECENT.lock().unwrap_or_else(PoisonError::into_inner);
    let recent = guard.get_or_insert_with(HashMap::new);
    let queue = recent.entry(prefix.to_string()).or_default();
    queue.push_back(id.clone());
    while queue.len() > KEPT {
        if let Some(old) = queue.pop_front().and_then(|o| CString::new(o).ok()) {
            // SAFETY: `old` is a NUL-terminated string the callee only reads.
            unsafe { od_picture_forget(old.as_ptr()) }
        }
    }
    Some(format!("image://pictures/{id}"))
}

/// Publishes a picture that stays until [`forget_all`] of its prefix (timeline thumbnails).
pub fn publish_pinned(prefix: &str, width: u32, height: u32, pixels: &[u8]) -> Option<String> {
    let id = format!("{prefix}{}", SERIAL.fetch_add(1, Ordering::Relaxed));
    publish_as(&id, width, height, pixels).then(|| format!("image://pictures/{id}"))
}

/// Drops every picture whose id starts with `prefix`.
pub fn forget_all(prefix: &str) {
    if let Ok(prefix) = CString::new(prefix) {
        // SAFETY: `prefix` is a NUL-terminated string the callee only reads.
        unsafe { od_picture_forget_prefix(prefix.as_ptr()) }
    }
}
