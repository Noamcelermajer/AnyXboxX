use std::ffi::c_void;
use std::ptr::null_mut;

use super::*;

fn factory(iid: &Guid) -> (HResult, *mut c_void) {
    let mut out = 0x1234 as *mut c_void;
    // SAFETY: valid GUID and output pointer.
    let hr = unsafe { DStorageGetFactory(iid, &mut out) };
    (hr, out)
}

fn vtable(object: *mut c_void) -> &'static FactoryVtable {
    // SAFETY: every factory handed out is the static `FACTORY`.
    unsafe { (*(object as *mut Factory)).vtable }
}

#[test]
fn the_factory_answers_its_iid_and_iunknown_only() {
    let (hr, object) = factory(&IID_FACTORY_X);
    assert_eq!((hr, object.is_null()), (S_OK, false));
    let (hr, same) = factory(&IID_IUNKNOWN);
    assert_eq!((hr, same), (S_OK, object));
    let other = Guid {
        data1: 1,
        ..IID_IUNKNOWN
    };
    assert_eq!(factory(&other), (E_NOINTERFACE, null_mut()));
}

#[test]
fn a_null_output_is_refused() {
    // SAFETY: a null output is part of the contract.
    assert_eq!(
        unsafe { DStorageGetFactory(&IID_FACTORY_X, null_mut()) },
        E_POINTER
    );
}

#[test]
fn unimplemented_slots_return_e_notimpl_and_clear_their_output() {
    let (_, object) = factory(&IID_FACTORY_X);
    let table = vtable(object);
    let this = object.cast::<Factory>();
    let mut out = 0x1234 as *mut c_void;
    // SAFETY: valid output pointer; the other pointers are unused by these slots.
    unsafe {
        assert_eq!(
            (table.create_queue)(this, null_mut(), &IID_FACTORY_X, &mut out),
            E_NOTIMPL
        );
        assert!(out.is_null());
        out = 0x1234 as *mut c_void;
        assert_eq!(
            (table.open_file)(this, null_mut(), &IID_FACTORY_X, &mut out),
            E_NOTIMPL
        );
        assert!(out.is_null());
        out = 0x1234 as *mut c_void;
        assert_eq!(
            (table.create_status_array)(this, 1, null_mut(), &IID_FACTORY_X, &mut out),
            E_NOTIMPL
        );
        assert!(out.is_null());
    }
    assert_eq!((table.set_cpu_affinity)(this, 1), E_NOTIMPL);
    assert_eq!((table.set_staging_buffer_size)(this, 0x200_0000), S_OK);
}

#[test]
fn the_vtable_has_the_console_slot_order() {
    // Slot 8 (+0x40) is SetStagingBufferSize, the one the title tests.
    assert_eq!(
        std::mem::size_of::<FactoryVtable>(),
        9 * std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::offset_of!(FactoryVtable, set_staging_buffer_size),
        0x40
    );
}
