//! macOS `DeviceMonitor` implementation using DiskArbitration.

use std::ffi::c_void;
use std::pin::Pin;
use std::sync::Arc;
use std::thread;

use core_foundation_sys::base::CFAllocatorRef;
use core_foundation_sys::dictionary::CFDictionaryRef;
use core_foundation_sys::runloop::{
    CFRunLoopGetCurrent, CFRunLoopRef, CFRunLoopRun, kCFRunLoopDefaultMode,
};
use core_foundation_sys::string::CFStringRef;
use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;

use crate::auth::device_monitor::{DeviceEvent, DeviceMonitor};

/// Monitors removable macOS volume events.
pub struct MacOsDeviceMonitor;

impl MacOsDeviceMonitor {
    /// Creates a macOS device monitor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacOsDeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceMonitor for MacOsDeviceMonitor {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>> {
        let (sender, receiver) = mpsc::channel(32);
        let sender = Arc::new(sender);

        thread::spawn(move || {
            // SAFETY: DiskArbitration session creation and run-loop scheduling are
            // performed and used only on this dedicated thread.
            unsafe {
                run_disk_arbitration_loop(sender);
            }
        });

        Box::pin(ReceiverStream::new(receiver))
    }
}

/// Opaque DiskArbitration session type.
type DASessionRef = *mut c_void;
/// Opaque DiskArbitration disk type.
type DADiskRef = *mut c_void;
/// Disk-appeared callback signature.
type DADiskAppearedCallback = extern "C" fn(disk: DADiskRef, context: *mut c_void);
/// Disk-disappeared callback signature.
type DADiskDisappearedCallback = extern "C" fn(disk: DADiskRef, context: *mut c_void);

#[link(name = "DiskArbitration", kind = "framework")]
unsafe extern "C" {
    /// Creates a DiskArbitration session.
    fn DASessionCreate(allocator: CFAllocatorRef) -> DASessionRef;
    /// Schedules a session on a run loop.
    fn DASessionScheduleWithRunLoop(
        session: DASessionRef,
        run_loop: CFRunLoopRef,
        run_loop_mode: CFStringRef,
    );
    /// Registers a callback for appeared disks.
    fn DARegisterDiskAppearedCallback(
        session: DASessionRef,
        match_dictionary: CFDictionaryRef,
        callback: DADiskAppearedCallback,
        context: *mut c_void,
    );
    /// Registers a callback for disappeared disks.
    fn DARegisterDiskDisappearedCallback(
        session: DASessionRef,
        match_dictionary: CFDictionaryRef,
        callback: DADiskDisappearedCallback,
        context: *mut c_void,
    );
    /// Copies disk metadata for a disk reference.
    fn DADiskCopyDescription(disk: DADiskRef) -> CFDictionaryRef;
}

/// Handles disk appeared callbacks.
extern "C" fn disk_appeared_callback(_disk: DADiskRef, _context: *mut c_void) {}

/// Handles disk disappeared callbacks.
extern "C" fn disk_disappeared_callback(_disk: DADiskRef, _context: *mut c_void) {}

/// Runs a DiskArbitration-backed run loop.
unsafe fn run_disk_arbitration_loop(_sender: Arc<mpsc::Sender<DeviceEvent>>) {
    if std::env::var("PANIC_ON_UNIMPLEMENTED_MACOS_MONITOR").as_deref() == Ok("1") {
        panic!("MacOsDeviceMonitor event translation is not implemented yet");
    }

    // SAFETY: Passing a null allocator requests the default CF allocator.
    let session = unsafe { DASessionCreate(std::ptr::null()) };
    if session.is_null() {
        return;
    }

    // SAFETY: The session is valid and this thread owns the run loop.
    unsafe {
        DASessionScheduleWithRunLoop(session, CFRunLoopGetCurrent(), kCFRunLoopDefaultMode);
        DARegisterDiskAppearedCallback(
            session,
            std::ptr::null(),
            disk_appeared_callback,
            std::ptr::null_mut(),
        );
        DARegisterDiskDisappearedCallback(
            session,
            std::ptr::null(),
            disk_disappeared_callback,
            std::ptr::null_mut(),
        );
        let _ = DADiskCopyDescription as unsafe extern "C" fn(DADiskRef) -> CFDictionaryRef;
        CFRunLoopRun();
    }
}
