//! d3d12_xs.dll in Rust: translation of D3D12.X (the console's DirectX 12). Port of
//! `runtime/d3d12_xs` (C++), same exports; checked by running `tests/test_d3d12_virtual_memory.py`
//! and the d3d12_xs tests of `tests/test_runtime_cpp.py` against this DLL (`tests/test_rust_dlls.py`).
//!
//! The root-signature functions are forwarded to the PC's d3d12.dll (`build.rs`). Not translated
//! yet: device creation (D3D12XboxCreateDevice) and the console ID3D12* interfaces, whose method
//! tables differ from the PC's; forwarding them as they are would crash the title.

use std::io::Write;

pub type HResult = i32;
pub const S_OK: HResult = 0;
pub const E_INVALIDARG: HResult = 0x8007_0057_u32 as i32;

/// The two leading fields of the console's VirtualMemoryConfiguration (layout from TomBox,
/// `crates/compat/d3d12_xs/src/vmem_x.rs`): flags, page_table_pages, then fields this function
/// never touches.
#[repr(C)]
pub struct VirtualMemoryConfigurationHead {
    pub flags: u32,
    pub page_table_pages: u32,
}

/// `HRESULT D3D12ConfigureVirtualMemoryX(VirtualMemoryConfiguration*)`, called by the title
/// before device creation (without it the title stops on "UNIMPLEMENTED
/// d3d12_xs.dll!D3D12ConfigureVirtualMemoryX").
///
/// On the real executable the title fills only page_table_pages (+4) and tests only the result;
/// the pointer may not cover 32 bytes. As Leonida measured (its shim writes nothing, #6750), only
/// flags and page_table_pages are read (and logged), no output is written. Null is refused.
///
/// # Safety
/// `description` is null or points at 8 readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn D3D12ConfigureVirtualMemoryX(
    description: *const VirtualMemoryConfigurationHead,
) -> HResult {
    // SAFETY: a non-null pointer covers the two leading fields per this function's contract.
    let Some(head) = (unsafe { description.as_ref() }) else {
        return E_INVALIDARG;
    };
    let mut stderr = std::io::stderr().lock();
    // A closed stderr must not fail the call: the line is then lost, nothing else.
    let _ = writeln!(
        stderr,
        "[d3d12_xs] D3D12ConfigureVirtualMemoryX: flags {}, pages {}",
        head.flags, head.page_table_pages
    );
    S_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_is_refused_and_a_head_is_accepted_without_writes() {
        // SAFETY: null is part of the contract.
        assert_eq!(
            unsafe { D3D12ConfigureVirtualMemoryX(std::ptr::null()) },
            E_INVALIDARG
        );
        let head = VirtualMemoryConfigurationHead {
            flags: 1,
            page_table_pages: 64,
        };
        // SAFETY: valid pointer to the two fields.
        assert_eq!(unsafe { D3D12ConfigureVirtualMemoryX(&head) }, S_OK);
        assert_eq!((head.flags, head.page_table_pages), (1, 64));
    }
}
