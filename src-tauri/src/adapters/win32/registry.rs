/*!
 * SOURCE OF TRUTH KEYWORDS: RegistryKey, registry key owner, RegOpenKeyExW, RegCreateKeyExW, RegGetValueW, RegSetValueExW, RegDeleteValueW, REG_SZ, REG_BINARY, RegCloseKey guard
 * WHAT:  RegistryKey: an open registry key closed on drop, with the handful of value operations Echo's adapters
 *        need: read and write a string or a binary value, and delete a value. Missing keys and values are `None`
 *        (or a no-op for a delete), not errors.
 * WHY:   The appearance watcher and the start-at-sign-in entry both read and write the user's registry; one owner
 *        keeps each `unsafe` call and its buffer sizes reviewed once (root CLAUDE.md §1). Values are read in two
 *        calls (size, then data) so a string of any length fits, and strings are written with their terminator as
 *        REG_SZ requires. Failures carry the key, the value and the Win32 code in the log-only detail.
 * WHERE: adapters/appearance/win32.rs (opens the Personalize key to watch it), adapters/startup/run_key.rs (the Run
 *        and StartupApproved values).
 */

use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR},
        System::Registry::{
            HKEY, REG_BINARY, REG_OPTION_NON_VOLATILE, REG_ROUTINE_FLAGS, REG_SAM_FLAGS, REG_SZ,
            RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW,
            RegGetValueW, RegOpenKeyExW, RegSetValueExW,
        },
    },
    core::{HSTRING, PCWSTR},
};

use crate::types::{AppError, PortError, PortResult};

/// An open registry key, closed on drop.
pub struct RegistryKey {
    key: HKEY,
    path: String,
}

impl RegistryKey {
    /// Opens `hive\subkey` with `access`; None when the key does not exist.
    pub fn open(hive: HKEY, subkey: &str, access: REG_SAM_FLAGS) -> PortResult<Option<Self>> {
        let mut key = HKEY::default();
        // SAFETY: `key` outlives the call and receives the opened handle; the name is a valid HSTRING.
        let status =
            unsafe { RegOpenKeyExW(hive, &HSTRING::from(subkey), None, access, &raw mut key) };
        match status {
            ERROR_SUCCESS => Ok(Some(Self {
                key,
                path: subkey.to_owned(),
            })),
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
            other => Err(failure("open", subkey, "", other)),
        }
    }

    /// Opens `hive\subkey` with `access`, creating it (and its parents) when it does not exist.
    pub fn create(hive: HKEY, subkey: &str, access: REG_SAM_FLAGS) -> PortResult<Self> {
        let mut key = HKEY::default();
        // SAFETY: `key` outlives the call and receives the handle; class and security are left to their defaults.
        let status = unsafe {
            RegCreateKeyExW(
                hive,
                &HSTRING::from(subkey),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                access,
                None,
                &raw mut key,
                None,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(Self {
                key,
                path: subkey.to_owned(),
            })
        } else {
            Err(failure("create", subkey, "", status))
        }
    }

    /// The raw handle, for calls such as change notifications; valid while `self` lives.
    pub const fn raw(&self) -> HKEY {
        self.key
    }

    /// The string value `name`; None when it does not exist.
    pub fn read_string(&self, name: &str) -> PortResult<Option<String>> {
        let Some(bytes) = self.read(name, RRF_RT_REG_SZ)? else {
            return Ok(None);
        };
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .take_while(|&unit| unit != 0)
            .collect();
        Ok(Some(String::from_utf16_lossy(&units)))
    }

    /// The binary value `name`; None when it does not exist.
    pub fn read_binary(&self, name: &str) -> PortResult<Option<Vec<u8>>> {
        self.read(name, RRF_RT_REG_BINARY)
    }

    /// Stores `value` as the REG_SZ value `name`.
    pub fn write_string(&self, name: &str, value: &str) -> PortResult<()> {
        let bytes: Vec<u8> = value
            .encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        self.write(name, REG_SZ, &bytes)
    }

    /// Stores `bytes` as the REG_BINARY value `name`.
    pub fn write_binary(&self, name: &str, bytes: &[u8]) -> PortResult<()> {
        self.write(name, REG_BINARY, bytes)
    }

    /// Deletes the value `name`; a value that does not exist is a no-op.
    pub fn delete_value(&self, name: &str) -> PortResult<()> {
        // SAFETY: the key is open for the lifetime of self; the name is a valid HSTRING.
        let status = unsafe { RegDeleteValueW(self.key, &HSTRING::from(name)) };
        match status {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            other => Err(failure("delete", &self.path, name, other)),
        }
    }

    fn read(&self, name: &str, kind: REG_ROUTINE_FLAGS) -> PortResult<Option<Vec<u8>>> {
        let value = HSTRING::from(name);
        let mut size = 0u32;
        // SAFETY: a null data pointer asks only for the size, written into `size`.
        let status = unsafe {
            RegGetValueW(
                self.key,
                PCWSTR::null(),
                &value,
                kind,
                None,
                None,
                Some(&raw mut size),
            )
        };
        match status {
            ERROR_SUCCESS => {}
            ERROR_FILE_NOT_FOUND => return Ok(None),
            other => return Err(failure("size", &self.path, name, other)),
        }
        let mut data = vec![0u8; usize::try_from(size).unwrap_or(0)];
        // SAFETY: `data` holds `size` bytes and outlives the call; `size` is updated to the bytes written.
        let status = unsafe {
            RegGetValueW(
                self.key,
                PCWSTR::null(),
                &value,
                kind,
                None,
                Some(data.as_mut_ptr().cast()),
                Some(&raw mut size),
            )
        };
        match status {
            ERROR_SUCCESS => {
                data.truncate(usize::try_from(size).unwrap_or(0));
                Ok(Some(data))
            }
            ERROR_FILE_NOT_FOUND => Ok(None),
            other => Err(failure("read", &self.path, name, other)),
        }
    }

    fn write(
        &self,
        name: &str,
        kind: windows::Win32::System::Registry::REG_VALUE_TYPE,
        bytes: &[u8],
    ) -> PortResult<()> {
        // SAFETY: the key is open for the lifetime of self; `bytes` is borrowed for the call only.
        let status =
            unsafe { RegSetValueExW(self.key, &HSTRING::from(name), None, kind, Some(bytes)) };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(failure("write", &self.path, name, status))
        }
    }
}

impl Drop for RegistryKey {
    fn drop(&mut self) {
        // SAFETY: the key was opened or created by this owner and is closed exactly once here.
        let _ = unsafe { RegCloseKey(self.key) };
    }
}

fn failure(action: &str, path: &str, name: &str, status: WIN32_ERROR) -> PortError {
    PortError::new(AppError::Internal).with_detail(format!(
        "registry {action} of {path}\\{name} failed: Win32 error {}",
        status.0
    ))
}

#[cfg(test)]
mod tests {
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, RegDeleteTreeW,
    };

    use super::*;

    /// A per-test key under HKCU\Software, deleted with everything below it on drop.
    struct ScratchKey(String);

    impl ScratchKey {
        fn new(name: &str) -> Self {
            Self(format!(
                r"Software\app.echo.desktop.tests\{name}-{}",
                std::process::id()
            ))
        }
    }

    impl Drop for ScratchKey {
        fn drop(&mut self) {
            // SAFETY: deletes only the scratch tree this test created.
            let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(self.0.as_str())) };
        }
    }

    #[test]
    fn values_round_trip_and_missing_ones_are_none() {
        let scratch = ScratchKey::new("registry-round-trip");
        assert!(
            RegistryKey::open(HKEY_CURRENT_USER, &scratch.0, KEY_READ)
                .unwrap()
                .is_none()
        );
        let key = RegistryKey::create(HKEY_CURRENT_USER, &scratch.0, KEY_READ | KEY_WRITE).unwrap();
        assert_eq!(key.read_string("text").unwrap(), None);
        key.write_string("text", r#""\host\Échø apps\echo.exe" --minimized"#)
            .unwrap();
        assert_eq!(
            key.read_string("text").unwrap().as_deref(),
            Some(r#""\host\Échø apps\echo.exe" --minimized"#)
        );
        key.write_binary("bytes", &[3, 0, 0, 0, 9]).unwrap();
        assert_eq!(key.read_binary("bytes").unwrap(), Some(vec![3, 0, 0, 0, 9]));
        key.delete_value("text").unwrap();
        key.delete_value("text").unwrap();
        assert_eq!(key.read_string("text").unwrap(), None);
    }
}
