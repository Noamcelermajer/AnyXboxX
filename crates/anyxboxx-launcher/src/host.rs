//! What the launcher runs on: Windows, or Proton/Wine (detected through Wine's `wine_get_version`
//! export of ntdll), and the GPU vendor of the main adapter, read from DXGI's PCI vendor id.

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG_SOFTWARE, DXGI_ERROR_NOT_FOUND,
    IDXGIFactory1,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::core::{PCSTR, w};

pub const VENDOR_NVIDIA: u32 = 0x10DE;
pub const VENDOR_AMD: u32 = 0x1002;
pub const VENDOR_INTEL: u32 = 0x8086;

/// Where the launcher runs. Proton and Wine are one host here: the profiles call it `linux`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Windows,
    WineOrProton,
}

/// The vendor of a GPU, from its PCI vendor id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    /// Another vendor id, kept for the report.
    Other(u32),
    /// No hardware adapter was found.
    Unknown,
}

impl GpuVendor {
    pub fn from_pci_id(id: u32) -> Self {
        match id {
            VENDOR_NVIDIA => GpuVendor::Nvidia,
            VENDOR_AMD => GpuVendor::Amd,
            VENDOR_INTEL => GpuVendor::Intel,
            other => GpuVendor::Other(other),
        }
    }

    pub fn label(self) -> String {
        match self {
            GpuVendor::Nvidia => "NVIDIA".to_string(),
            GpuVendor::Amd => "AMD".to_string(),
            GpuVendor::Intel => "Intel".to_string(),
            GpuVendor::Other(id) => format!("vendor 0x{id:04X}"),
            GpuVendor::Unknown => "unknown".to_string(),
        }
    }
}

/// Pure decision of the host from the presence of Wine's export, for tests.
pub fn host_from_exports(has_wine_version: bool) -> Host {
    if has_wine_version {
        Host::WineOrProton
    } else {
        Host::Windows
    }
}

/// Detects the host: ntdll exports `wine_get_version` under Wine and Proton only.
pub fn detect_host() -> Host {
    // SAFETY: the module name is a static wide string; a missing module yields an error, not UB.
    let ntdll: Option<HMODULE> = unsafe { GetModuleHandleW(w!("ntdll.dll")) }.ok();
    let has_wine_version = ntdll.is_some_and(|module| {
        // SAFETY: a valid module handle and a static NUL-terminated name.
        unsafe { GetProcAddress(module, PCSTR(c"wine_get_version".as_ptr().cast())) }.is_some()
    });
    host_from_exports(has_wine_version)
}

/// One DXGI adapter, as far as the launcher uses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    pub name: String,
    pub vendor_id: u32,
    pub dedicated_video_memory: u64,
    pub software: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("DXGI factory: {0}")]
    Factory(windows::core::Error),
    #[error("DXGI adapter {0}: {1}")]
    Adapter(u32, windows::core::Error),
}

/// Lists the DXGI adapters, software ones included (the caller filters).
pub fn enumerate_adapters() -> Result<Vec<Adapter>, GpuError> {
    // SAFETY: no argument; the returned factory is owned and released by windows-rs.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.map_err(GpuError::Factory)?;
    let mut adapters = Vec::new();
    for index in 0u32.. {
        // SAFETY: a factory created above and an index.
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => return Err(GpuError::Adapter(index, error)),
        };
        // SAFETY: a valid adapter interface.
        let desc: DXGI_ADAPTER_DESC1 =
            unsafe { adapter.GetDesc1() }.map_err(|e| GpuError::Adapter(index, e))?;
        adapters.push(Adapter {
            name: wide_to_string(&desc.Description),
            vendor_id: desc.VendorId,
            dedicated_video_memory: desc.DedicatedVideoMemory as u64,
            software: desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0,
        });
    }
    Ok(adapters)
}

/// The main GPU: the hardware adapter with the most dedicated video memory.
pub fn main_adapter(adapters: &[Adapter]) -> Option<&Adapter> {
    adapters
        .iter()
        .filter(|a| !a.software)
        .max_by_key(|a| a.dedicated_video_memory)
}

/// The vendor of the main GPU, or `Unknown` when there is none.
pub fn vendor_of(adapters: &[Adapter]) -> GpuVendor {
    main_adapter(adapters).map_or(GpuVendor::Unknown, |a| GpuVendor::from_pci_id(a.vendor_id))
}

fn wide_to_string(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(vendor_id: u32, memory: u64, software: bool) -> Adapter {
        Adapter {
            name: format!("gpu {vendor_id:x}"),
            vendor_id,
            dedicated_video_memory: memory,
            software,
        }
    }

    #[test]
    fn pci_ids_map_to_vendors() {
        assert_eq!(GpuVendor::from_pci_id(0x10DE), GpuVendor::Nvidia);
        assert_eq!(GpuVendor::from_pci_id(0x1002), GpuVendor::Amd);
        assert_eq!(GpuVendor::from_pci_id(0x8086), GpuVendor::Intel);
        assert_eq!(GpuVendor::from_pci_id(0x1414), GpuVendor::Other(0x1414));
    }

    #[test]
    fn wine_export_means_proton_or_wine() {
        assert_eq!(host_from_exports(true), Host::WineOrProton);
        assert_eq!(host_from_exports(false), Host::Windows);
    }

    #[test]
    fn main_adapter_skips_software_and_takes_the_largest_memory() {
        let adapters = [
            adapter(0x1414, 0, true),
            adapter(0x8086, 2 << 30, false),
            adapter(0x10DE, 12 << 30, false),
        ];
        assert_eq!(vendor_of(&adapters), GpuVendor::Nvidia);
    }

    #[test]
    fn only_software_adapters_means_unknown_vendor() {
        assert_eq!(vendor_of(&[adapter(0x1414, 0, true)]), GpuVendor::Unknown);
        assert_eq!(vendor_of(&[]), GpuVendor::Unknown);
    }

    #[test]
    fn wide_names_stop_at_the_first_nul() {
        let wide: Vec<u16> = "RTX 4070\0garbage".encode_utf16().collect();
        assert_eq!(wide_to_string(&wide), "RTX 4070");
    }
}
