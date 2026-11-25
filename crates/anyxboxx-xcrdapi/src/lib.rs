//! XCRDAPI.dll in Rust: `DStorageGetFactory` (first-boot note, W7). Port of
//! `runtime/xcrdapi/xcrdapi.cpp`, same export and same behaviour; checked by running
//! `tests/test_xcrdapi.py` against this DLL (`tests/test_rust_dlls.py`).
//!
//! The title asks for a DirectStorage factory the first time it uses a file. The factory follows
//! the console layout (IDStorageFactoryX, slots 0 to 8: IUnknown, CreateQueue, OpenFile,
//! CreateStatusArray, SetCpuAffinity, SetDebugFlags, SetStagingBufferSize), slot table as read in
//! TomBox (`crates/compat/xcrdapi/src/objects.rs`, GPL-3.0-or-later).
//!
//! Done: the factory is returned for its IID and for IUnknown; SetStagingBufferSize (slot 8,
//! +0x40) returns S_OK, which the title tests, without allocating anything (no file reading
//! exists yet, so no staging size is used). Everything else returns E_NOTIMPL with its output
//! set to NULL, and is logged.

use std::ffi::c_void;
use std::io::Write;

pub type HResult = i32;

pub const S_OK: HResult = 0;
pub const E_NOTIMPL: HResult = 0x8000_4001_u32 as i32;
pub const E_NOINTERFACE: HResult = 0x8000_4002_u32 as i32;
pub const E_POINTER: HResult = 0x8000_4003_u32 as i32;

/// A COM interface identifier, laid out like Windows' `GUID`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

/// {FB3B54AC-BF82-463A-B777-73195837864D}: the console's IDStorageFactoryX.
pub const IID_FACTORY_X: Guid = Guid {
    data1: 0xfb3b_54ac,
    data2: 0xbf82,
    data3: 0x463a,
    data4: [0xb7, 0x77, 0x73, 0x19, 0x58, 0x37, 0x86, 0x4d],
};
/// {00000000-0000-0000-C000-000000000046}.
pub const IID_IUNKNOWN: Guid = Guid {
    data1: 0,
    data2: 0,
    data3: 0,
    data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
};

#[repr(C)]
struct Factory {
    vtable: &'static FactoryVtable,
}

#[repr(C)]
struct FactoryVtable {
    query_interface:
        unsafe extern "system" fn(*mut Factory, *const Guid, *mut *mut c_void) -> HResult,
    add_ref: extern "system" fn(*mut Factory) -> u32,
    release: extern "system" fn(*mut Factory) -> u32,
    create_queue: unsafe extern "system" fn(
        *mut Factory,
        *const c_void,
        *const Guid,
        *mut *mut c_void,
    ) -> HResult,
    open_file: unsafe extern "system" fn(
        *mut Factory,
        *const u16,
        *const Guid,
        *mut *mut c_void,
    ) -> HResult,
    create_status_array: unsafe extern "system" fn(
        *mut Factory,
        u32,
        *const u8,
        *const Guid,
        *mut *mut c_void,
    ) -> HResult,
    set_cpu_affinity: extern "system" fn(*mut Factory, u64) -> HResult,
    set_debug_flags: extern "system" fn(*mut Factory, u32),
    set_staging_buffer_size: extern "system" fn(*mut Factory, u32) -> HResult,
}

static VTABLE: FactoryVtable = FactoryVtable {
    query_interface,
    add_ref,
    release,
    create_queue,
    open_file,
    create_status_array,
    set_cpu_affinity,
    set_debug_flags,
    set_staging_buffer_size,
};

/// Wrapper so the static factory can hold a raw object address shared by every caller.
struct Shared(std::cell::UnsafeCell<Factory>);
// SAFETY: the factory is never written after initialisation; its only field is a shared
// reference to an immutable static table.
unsafe impl Sync for Shared {}

static FACTORY: Shared = Shared(std::cell::UnsafeCell::new(Factory { vtable: &VTABLE }));

/// Runtime log on stderr, shared with AnyXboxX (same channel as the C++ `anyxboxx::Log`).
fn log(message: &str) {
    let mut stderr = std::io::stderr().lock();
    // A closed stderr must not crash the title: the message is then lost, nothing else.
    let _ = writeln!(stderr, "{message}");
    let _ = stderr.flush();
}

/// Clears `*out` when the caller gave an output pointer.
///
/// # Safety
/// `out` is null or valid for one pointer-sized write.
unsafe fn clear(out: *mut *mut c_void) {
    if !out.is_null() {
        // SAFETY: non-null and valid per this function's contract.
        unsafe { *out = std::ptr::null_mut() };
    }
}

/// # Safety
/// `riid` is null or points at a GUID; `out` is null or valid for one pointer write.
unsafe extern "system" fn query_interface(
    this: *mut Factory,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> HResult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `out` is non-null and valid per the COM contract.
    unsafe { *out = std::ptr::null_mut() };
    // SAFETY: a non-null `riid` points at a GUID per the COM contract.
    let iid = unsafe { riid.as_ref() };
    match iid {
        Some(iid) if *iid == IID_FACTORY_X || *iid == IID_IUNKNOWN => {
            // SAFETY: `out` is valid, checked above.
            unsafe { *out = this.cast() };
            S_OK
        }
        _ => E_NOINTERFACE,
    }
}

// The factory is a static object: the count only honours the COM contract.
extern "system" fn add_ref(_: *mut Factory) -> u32 {
    1
}

extern "system" fn release(_: *mut Factory) -> u32 {
    1
}

unsafe extern "system" fn create_queue(
    _: *mut Factory,
    _: *const c_void,
    _: *const Guid,
    out: *mut *mut c_void,
) -> HResult {
    // SAFETY: COM contract of CreateQueue: `out` is null or writable.
    unsafe { clear(out) };
    log("[XCRDAPI] IDStorageFactoryX::CreateQueue: not implemented (E_NOTIMPL)");
    E_NOTIMPL
}

unsafe extern "system" fn open_file(
    _: *mut Factory,
    _: *const u16,
    _: *const Guid,
    out: *mut *mut c_void,
) -> HResult {
    // SAFETY: COM contract of OpenFile: `out` is null or writable.
    unsafe { clear(out) };
    log("[XCRDAPI] IDStorageFactoryX::OpenFile: file reading not implemented (E_NOTIMPL)");
    E_NOTIMPL
}

unsafe extern "system" fn create_status_array(
    _: *mut Factory,
    _: u32,
    _: *const u8,
    _: *const Guid,
    out: *mut *mut c_void,
) -> HResult {
    // SAFETY: COM contract of CreateStatusArray: `out` is null or writable.
    unsafe { clear(out) };
    log("[XCRDAPI] IDStorageFactoryX::CreateStatusArray: not implemented (E_NOTIMPL)");
    E_NOTIMPL
}

extern "system" fn set_cpu_affinity(_: *mut Factory, mask: u64) -> HResult {
    log(&format!(
        "[XCRDAPI] IDStorageFactoryX::SetCpuAffinity mask={mask:x}: not implemented (E_NOTIMPL)"
    ));
    E_NOTIMPL
}

extern "system" fn set_debug_flags(_: *mut Factory, flags: u32) {
    log(&format!(
        "[XCRDAPI] IDStorageFactoryX::SetDebugFlags flags={flags:x}: logged, no effect"
    ));
}

extern "system" fn set_staging_buffer_size(_: *mut Factory, size: u32) -> HResult {
    // No staging buffer: no file reading exists yet. The size is logged.
    log(&format!(
        "[XCRDAPI] IDStorageFactoryX::SetStagingBufferSize size={size:x}: ignored (no staging buffer)"
    ));
    S_OK
}

/// `HRESULT DStorageGetFactory(REFIID riid, void** ppv)`: the factory for its IID or IUnknown.
///
/// # Safety
/// `riid` is null or points at a GUID; `out` is null or valid for one pointer write.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DStorageGetFactory(
    riid: *const Guid,
    out: *mut *mut c_void,
) -> HResult {
    // SAFETY: same contract as this function.
    unsafe { query_interface(FACTORY.0.get(), riid, out) }
}

#[cfg(test)]
mod tests;
