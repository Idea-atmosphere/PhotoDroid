//! The JNI bridge to `PhotoCraftActivity`: the one place this crate calls into Java.
//!
//! android-activity gives `android_main` the NativeActivity. We keep a pointer to it and call its
//! `photocraftCall(String, String) -> String` for every request (the protocol is in
//! `protocol.rs`, and the Java side is `PhotoCraftActivity.java`).
//!
//! This module holds the crate's only `unsafe` block, which the workspace's `unsafe_code = "forbid"`
//! would otherwise reject: wrapping the activity's global reference (see the SAFETY comment).

#![allow(unsafe_code)]

use std::ffi::c_void;
use std::sync::OnceLock;

use jni::objects::{JObject, JString, JValue};
use jni::{JavaVM, jni_sig, jni_str};

/// The activity's global reference, stored as an address so it can live in a `static`.
static ACTIVITY: OnceLock<usize> = OnceLock::new();

/// Remember the activity. Called once from `android_main`; later calls change nothing.
pub fn set_activity(activity: *mut c_void) {
    let _ = ACTIVITY.set(activity as usize);
}

/// Take ownership of a file descriptor the activity detached from a `ParcelFileDescriptor` and
/// sent over (see `Event::Descriptor`). The returned `File` closes it when dropped.
pub fn take_descriptor(fd: i32) -> std::fs::File {
    use std::os::fd::FromRawFd;
    // SAFETY: the activity detached `fd` from its ParcelFileDescriptor and reports the number
    // exactly once, so nothing else in this process owns it. From here on the File owns it.
    unsafe { std::fs::File::from_raw_fd(fd) }
}

/// Send one request to the activity and return its raw reply (see `protocol::parse_reply`).
pub fn call(command: &str, argument: &str) -> Result<String, String> {
    let activity = *ACTIVITY.get().ok_or("the Android activity is not available")?;
    let vm = JavaVM::singleton().map_err(|e| e.to_string())?;
    vm.attach_current_thread(|env| -> jni::errors::Result<String> {
        // SAFETY: `activity` is the global reference android-activity created for our
        // NativeActivity, and it stays valid for the whole of `android_main`. `JObject` never
        // deletes what it wraps, so wrapping it without owning it here is sound.
        let activity = unsafe { JObject::from_raw(env, activity as jni::sys::jobject) };
        let request = env.new_string(command)?;
        let argument = env.new_string(argument)?;
        let reply = env
            .call_method(
                &activity,
                jni_str!("photocraftCall"),
                jni_sig!((command: java.lang.String, argument: java.lang.String) -> java.lang.String),
                &[JValue::from(&request), JValue::from(&argument)],
            )?
            .l()?;
        let reply = env.cast_local::<JString>(reply)?;
        reply.try_to_string(env)
    })
    .map_err(|e| e.to_string())
}
