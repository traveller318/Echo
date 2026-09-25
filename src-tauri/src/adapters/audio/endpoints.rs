/*!
 * SOURCE OF TRUTH KEYWORDS: MMDevice endpoints, IMMDeviceEnumerator, endpoint enumerator name, BTHENUM, BTHHFENUM, Bluetooth endpoint detection, audio transport
 * WHAT:  Core Audio helpers for the capture adapter: `enumerator()` creates the MMDevice enumerator, and
 *        `bus_of(enumerator, endpoint_id)` reads which bus driver enumerated an endpoint's device (for example
 *        `USB`, `HDAUDIO`, `BTHENUM`, `BTHHFENUM`); `is_bluetooth_bus` classifies that name.
 * WHY:   cpal classifies an endpoint's connection but only recognises Bluetooth's A2DP bus (`BTHENUM`), while a
 *        headset microphone is on the hands-free bus (`BTHHFENUM`) and LE Audio on its own `BTH…` bus, which is the
 *        very case 05 W11 is about. Reading DEVPKEY_Device_EnumeratorName directly catches every Bluetooth bus. The
 *        caller must have COM entered on the thread (ComScope). The PROPVARIANT is always cleared and the string
 *        freed, even on a failed conversion; any failure simply means "not known", never an error for the caller.
 * WHERE: adapters/audio/cpal_wasapi.rs (device listing and the stream it opens); device_watch.rs (enumerator).
 */

use windows::{
    Win32::{
        Devices::Properties::DEVPKEY_Device_EnumeratorName,
        Foundation::PROPERTYKEY,
        Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator},
        System::Com::{
            CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ,
            StructuredStorage::{PropVariantClear, PropVariantToStringAlloc},
        },
    },
    core::HSTRING,
};

/// The MMDevice enumerator; COM must be entered on this thread.
pub fn enumerator() -> windows::core::Result<IMMDeviceEnumerator> {
    // SAFETY: a registered in-process class, no aggregation; COM is entered by the caller.
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
}

/// The bus driver that enumerated endpoint `endpoint_id`'s device; None when it cannot be read.
pub fn bus_of(enumerator: &IMMDeviceEnumerator, endpoint_id: &str) -> Option<String> {
    let id = HSTRING::from(endpoint_id);
    let key = PROPERTYKEY {
        fmtid: DEVPKEY_Device_EnumeratorName.fmtid,
        pid: DEVPKEY_Device_EnumeratorName.pid,
    };
    // SAFETY: `id` outlives the call; the returned interfaces are owned (released on drop).
    let store = unsafe {
        enumerator
            .GetDevice(&id)
            .and_then(|device| device.OpenPropertyStore(STGM_READ))
    }
    .ok()?;
    // SAFETY: `key` is a valid PROPERTYKEY for the duration of the call; the PROPVARIANT is owned by us from here.
    let mut value = unsafe { store.GetValue(&key) }.ok()?;
    // SAFETY: `value` is an initialized PROPVARIANT; the returned string is ours to free with CoTaskMemFree.
    let text = unsafe { PropVariantToStringAlloc(&value) };
    // SAFETY: `value` came from GetValue and is cleared exactly once; a failed clear leaks at most one string.
    let _ = unsafe { PropVariantClear(&mut value) };
    let text = text.ok()?;
    // SAFETY: `text` is a valid null-terminated wide string until it is freed below.
    let name = unsafe { text.to_string() }.ok();
    // SAFETY: allocated by PropVariantToStringAlloc with the COM allocator and freed exactly once.
    unsafe { CoTaskMemFree(Some(text.0.cast_const().cast())) };
    name
}

/// The bus is one of Windows' Bluetooth buses (classic `BTHENUM`, hands-free `BTHHFENUM`, LE Audio).
pub fn is_bluetooth_bus(bus: &str) -> bool {
    bus.get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("BTH"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::win32::ComScope;

    #[test]
    fn bluetooth_buses_are_recognised_by_their_prefix() {
        for bus in ["BTHENUM", "BTHHFENUM", "BTHLEDEVICE", "bthenum"] {
            assert!(is_bluetooth_bus(bus), "{bus}");
        }
        for bus in ["USB", "HDAUDIO", "SWD", "MMDEVAPI", "BT", ""] {
            assert!(!is_bluetooth_bus(bus), "{bus}");
        }
    }

    #[test]
    fn an_unknown_endpoint_has_no_bus() {
        let _com = ComScope::shared();
        let enumerator = enumerator().unwrap();
        assert_eq!(
            bus_of(
                &enumerator,
                "{0.0.1.00000000}.{00000000-0000-0000-0000-000000000000}"
            ),
            None
        );
    }
}
