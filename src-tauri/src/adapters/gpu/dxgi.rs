/*!
 * SOURCE OF TRUTH KEYWORDS: DxgiGraphicsAdapters, DXGI adapter enumeration, EnumAdapters1, EnumAdapterByGpuPreference, D3D12CreateDevice support check, UMD driver version, CheckInterfaceSupport, software adapter filter
 * WHAT:  DxgiGraphicsAdapters: the GraphicsAdapters port on DXGI. Enumerates adapters with `EnumAdapters1`, keeps the
 *        hardware ones that can create a Direct3D 12 device at feature level 11_0, reads each one's name, PCI ids,
 *        dedicated memory and user-mode driver version, and orders them high-performance first.
 * WHY:   DirectML opens a GPU by its `EnumAdapters1` index (`device_id`), so the ordinal recorded here is exactly the
 *        number the ONNX session takes (adapters/onnx/session.rs). ONNX Runtime's DirectML provider needs a D3D12
 *        device at feature level 11_0 and refuses software adapters, so the same test filters the list: a null
 *        device pointer asks D3D12CreateDevice whether it *could* create one without creating it. The order comes
 *        from IDXGIFactory6::EnumAdapterByGpuPreference(HIGH_PERFORMANCE) (Windows 10 1803+; Echo needs 1809), so a
 *        laptop's discrete GPU comes before its integrated one; without that interface the enumeration order is
 *        kept. The driver version is the user-mode driver's, which `CheckInterfaceSupport(IDXGIDevice)` returns as
 *        four 16-bit parts (the documented way; the WMI string is a different, marketing-dependent value). DXGI
 *        needs no COM apartment. Nothing is cached: GPUs and drivers change while Echo runs.
 * WHERE: Built by app/bootstrap into the ASR worker's accelerator picker (pipeline/asr/accelerator.rs), which calls
 *        `list` on the loader thread at every engine load.
 */

use windows::{
    Win32::{
        Foundation::LUID,
        Graphics::{
            Direct3D::D3D_FEATURE_LEVEL_11_0,
            Direct3D12::{D3D12CreateDevice, ID3D12Device},
            Dxgi::{
                CreateDXGIFactory1, DXGI_ADAPTER_DESC1, DXGI_ADAPTER_FLAG_SOFTWARE,
                DXGI_ERROR_NOT_FOUND, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, IDXGIAdapter1,
                IDXGIDevice, IDXGIFactory1, IDXGIFactory6,
            },
        },
    },
    core::Interface,
};

use crate::{
    ports::GraphicsAdapters,
    types::{AppError, ByteCount, GpuAdapter, PortError, PortResult},
};

/// More adapters than any machine has; stops a misbehaving enumeration.
const MAX_ADAPTERS: u32 = 64;

/// The DirectX 12 GPUs, read from DXGI.
#[derive(Debug, Default)]
pub struct DxgiGraphicsAdapters;

impl DxgiGraphicsAdapters {
    pub const fn new() -> Self {
        Self
    }
}

impl GraphicsAdapters for DxgiGraphicsAdapters {
    fn list(&self) -> PortResult<Vec<GpuAdapter>> {
        // SAFETY: CreateDXGIFactory1 has no preconditions; the factory is released when dropped.
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
            .map_err(|error| failure("create a DXGI factory", &error))?;
        let mut found = Vec::new();
        for ordinal in 0..MAX_ADAPTERS {
            // SAFETY: the factory is valid for the call; an ordinal past the last adapter is DXGI_ERROR_NOT_FOUND.
            let adapter = match unsafe { factory.EnumAdapters1(ordinal) } {
                Ok(adapter) => adapter,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => return Err(failure("enumerate graphics adapters", &error)),
            };
            match describe(&adapter, ordinal) {
                Ok(Some(described)) => found.push(described),
                Ok(None) => {}
                // One unreadable adapter must not hide the others.
                Err(error) => {
                    tracing::warn!(ordinal, detail = error.detail(), "graphics adapter skipped")
                }
            }
        }
        let preferred = preference_order(&factory);
        Ok(order_by_preference(found, &preferred))
    }
}

/// The adapter as a DX12-capable GPU with its LUID key; None for a software adapter or one without DX12.
fn describe(adapter: &IDXGIAdapter1, ordinal: u32) -> PortResult<Option<(u64, GpuAdapter)>> {
    // SAFETY: the adapter is valid for the call; GetDesc1 fills a plain struct.
    let desc: DXGI_ADAPTER_DESC1 = unsafe { adapter.GetDesc1() }
        .map_err(|error| failure("describe a graphics adapter", &error))?;
    if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0.cast_unsigned() != 0 {
        return Ok(None);
    }
    // SAFETY: a null device pointer only asks whether a feature level 11_0 device could be created (S_FALSE);
    // nothing is created or returned.
    let supports_d3d12 = unsafe {
        D3D12CreateDevice(
            adapter,
            D3D_FEATURE_LEVEL_11_0,
            std::ptr::null_mut::<Option<ID3D12Device>>(),
        )
    }
    .is_ok();
    if !supports_d3d12 {
        return Ok(None);
    }
    // SAFETY: the adapter is valid and the IID outlives the call; the result is the packed driver version.
    let driver = unsafe { adapter.CheckInterfaceSupport(&IDXGIDevice::IID) }
        .map_err(|error| failure("read a graphics driver version", &error))?;
    Ok(Some((
        luid_key(desc.AdapterLuid),
        GpuAdapter {
            ordinal,
            name: adapter_name(&desc.Description),
            vendor_id: desc.VendorId,
            device_id: desc.DeviceId,
            driver_version: driver_version(driver.cast_unsigned()),
            dedicated_memory: ByteCount::new(
                u64::try_from(desc.DedicatedVideoMemory).unwrap_or(u64::MAX),
            ),
        },
    )))
}

/// LUIDs of the hardware adapters in high-performance order; empty when the OS cannot say.
fn preference_order(factory: &IDXGIFactory1) -> Vec<u64> {
    let Ok(factory) = factory.cast::<IDXGIFactory6>() else {
        return Vec::new();
    };
    let mut order = Vec::new();
    for index in 0..MAX_ADAPTERS {
        // SAFETY: the factory is valid; an index past the last adapter is an error, which ends the loop.
        let Ok(adapter) = (unsafe {
            factory.EnumAdapterByGpuPreference::<IDXGIAdapter1>(
                index,
                DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
            )
        }) else {
            break;
        };
        // SAFETY: as in `describe`.
        if let Ok(desc) = unsafe { adapter.GetDesc1() } {
            order.push(luid_key(desc.AdapterLuid));
        }
    }
    order
}

/// Sorts adapters by their position in `preferred` (unlisted ones last, in enumeration order).
fn order_by_preference(mut found: Vec<(u64, GpuAdapter)>, preferred: &[u64]) -> Vec<GpuAdapter> {
    found.sort_by_key(|(luid, adapter)| {
        let rank = preferred
            .iter()
            .position(|key| key == luid)
            .unwrap_or(usize::MAX);
        (rank, adapter.ordinal)
    });
    found.into_iter().map(|(_, adapter)| adapter).collect()
}

fn luid_key(luid: LUID) -> u64 {
    (u64::from(luid.HighPart.cast_unsigned()) << 32) | u64::from(luid.LowPart)
}

/// The null-terminated UTF-16 description, trimmed.
fn adapter_name(description: &[u16]) -> String {
    let end = description
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(description.len());
    String::from_utf16_lossy(&description[..end])
        .trim()
        .to_owned()
}

/// The UMD version packed as four 16-bit parts, e.g. `32.0.101.7076`.
fn driver_version(packed: u64) -> String {
    let part = |shift: u32| (packed >> shift) & 0xFFFF;
    format!("{}.{}.{}.{}", part(48), part(32), part(16), part(0))
}

fn failure(action: &str, error: &windows::core::Error) -> PortError {
    PortError::new(AppError::Internal).with_detail(format!("DXGI could not {action}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(ordinal: u32) -> GpuAdapter {
        GpuAdapter {
            ordinal,
            name: format!("GPU {ordinal}"),
            vendor_id: 0x10de,
            device_id: 1,
            driver_version: String::from("1.0.0.0"),
            dedicated_memory: ByteCount::new(0),
        }
    }

    #[test]
    fn driver_versions_unpack_into_four_parts() {
        let packed = (32_u64 << 48) | (101 << 16) | 7076;
        assert_eq!(driver_version(packed), "32.0.101.7076");
        assert_eq!(driver_version(0), "0.0.0.0");
    }

    #[test]
    fn names_stop_at_the_terminator() {
        let mut description = [0_u16; 128];
        for (slot, unit) in description
            .iter_mut()
            .zip("Intel(R) UHD Graphics ".encode_utf16())
        {
            *slot = unit;
        }
        assert_eq!(adapter_name(&description), "Intel(R) UHD Graphics");
        assert_eq!(adapter_name(&[]), "");
    }

    #[test]
    fn the_preferred_gpu_comes_first_and_unknown_ones_keep_their_order() {
        let found = vec![(10, gpu(0)), (20, gpu(1)), (30, gpu(2))];
        let ordered = order_by_preference(found, &[20, 10]);
        let ordinals: Vec<u32> = ordered.iter().map(|adapter| adapter.ordinal).collect();
        assert_eq!(ordinals, [1, 0, 2]);
        let unordered = order_by_preference(vec![(1, gpu(1)), (2, gpu(0))], &[]);
        assert_eq!(unordered[0].ordinal, 0);
    }

    #[test]
    fn luids_pack_both_halves() {
        let key = luid_key(LUID {
            LowPart: 7,
            HighPart: 1,
        });
        assert_eq!(key, (1 << 32) | 7);
    }

    /// Lists this machine's GPUs: every entry is a named hardware adapter with a driver version.
    #[test]
    fn this_machine_lists_well_formed_hardware_adapters() {
        let adapters = DxgiGraphicsAdapters::new().list().unwrap();
        for adapter in &adapters {
            assert!(!adapter.name.is_empty(), "{adapter:?}");
            assert_ne!(adapter.driver_version, "0.0.0.0", "{adapter:?}");
        }
        let mut ordinals: Vec<u32> = adapters.iter().map(|adapter| adapter.ordinal).collect();
        ordinals.sort_unstable();
        ordinals.dedup();
        assert_eq!(ordinals.len(), adapters.len(), "each adapter once");
    }
}
