/*!
 * SOURCE OF TRUTH KEYWORDS: Win32PrivacyConsent, microphone privacy, CapabilityAccessManager, ConsentStore, NonPackaged, RegGetValueW, mic blocked
 * WHAT:  Win32PrivacyConsent: answers PrivacyConsent::microphone from the Windows privacy consent store
 *        (`CapabilityAccessManager\ConsentStore\microphone`), `Denied` when any switch that covers Echo is off.
 * WHY:   With mic privacy off, cpal opens the device but delivers silence (05 W13), so the registry Microphone
 *        permission reads the consent instead. Echo is an unpackaged desktop app, so three switches apply, and
 *        any one set to `Deny` blocks it: the device-wide switch (HKLM), the user's "Microphone access" (HKCU)
 *        and "Let desktop apps access your microphone" (HKCU `NonPackaged`). Newer Windows builds can also hold a
 *        per-app `Deny` under `NonPackaged\<exe path with \ replaced by #>`, so that key is read too. A missing
 *        key or value means the switch was never touched, which Windows treats as allowed. Any other read
 *        failure is returned with its Win32 code as log detail, never guessed (registry::permissions), unless
 *        another switch already says `Deny`, which decides the answer on its own. The keys are registry paths,
 *        not file paths, and the exe path comes from `current_exe()` (05 W23).
 * WHERE: Built by app/bootstrap into CommandCtx; asked by registry::permissions (factory preflight, onboarding).
 */

use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW,
        },
    },
    core::HSTRING,
};

use crate::{
    ports::PrivacyConsent,
    types::{AppError, PermissionState, PortError, PortResult},
};

const MICROPHONE_STORE: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";
const DESKTOP_APPS: &str = "NonPackaged";
const CONSENT_VALUE: &str = "Value";
const DENY: &str = "Deny";
/// Consent values are `Allow` / `Deny`; anything longer is not `Deny`.
const VALUE_CAPACITY: usize = 16;

/// Reads the Windows microphone privacy switches that apply to this executable.
pub struct Win32PrivacyConsent {
    /// Subkeys of the per-user switches (HKCU); the device-wide switch is always `MICROPHONE_STORE` in HKLM.
    /// Key names are kept as text because `HKEY` is a raw handle and the port must be `Send + Sync`.
    user_switches: Vec<String>,
}

impl Win32PrivacyConsent {
    pub fn new() -> Self {
        let desktop_apps = format!(r"{MICROPHONE_STORE}\{DESKTOP_APPS}");
        let mut user_switches = vec![MICROPHONE_STORE.to_owned(), desktop_apps.clone()];
        // Without an exe path the per-app switch cannot be named; the three global switches still apply.
        if let Ok(exe) = std::env::current_exe() {
            let app_key = exe.to_string_lossy().replace('\\', "#");
            user_switches.push(format!(r"{desktop_apps}\{app_key}"));
        }
        Self { user_switches }
    }
}

impl Default for Win32PrivacyConsent {
    fn default() -> Self {
        Self::new()
    }
}

impl PrivacyConsent for Win32PrivacyConsent {
    fn microphone(&self) -> PortResult<PermissionState> {
        let device = read_switch(HKEY_LOCAL_MACHINE, MICROPHONE_STORE);
        let user = self
            .user_switches
            .iter()
            .map(|subkey| read_switch(HKEY_CURRENT_USER, subkey));
        combine(std::iter::once(device).chain(user))
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: consent switch combine, any deny blocks, unset switch allowed, consent read failure
 * WHAT:  Folds the switch reads into one answer: `Denied` if any switch says so, else the first read failure,
 *        else `Granted` (an unset switch, `None`, counts as allowed).
 * WHY:   A `Deny` is certain even when another key could not be read; a failure without any `Deny` is not, so it
 *        is returned for the caller to log and decide. Pure, so the rule is tested without touching Windows.
 * WHERE: Win32PrivacyConsent::microphone; tests below.
 */
fn combine(
    reads: impl IntoIterator<Item = PortResult<Option<PermissionState>>>,
) -> PortResult<PermissionState> {
    let mut failure = None;
    for read in reads {
        match read {
            Ok(Some(PermissionState::Denied)) => return Ok(PermissionState::Denied),
            Ok(Some(PermissionState::Granted) | None) => {}
            Err(error) => {
                failure.get_or_insert(error);
            }
        }
    }
    failure.map_or(Ok(PermissionState::Granted), Err)
}

/**
 * SOURCE OF TRUTH KEYWORDS: RegGetValueW consent read, REG_SZ Value, missing key allowed, Win32 error detail
 * WHAT:  Reads the `Value` string of one consent switch: `Some(Denied)` for `Deny`, `Some(Granted)` for any other
 *        value, `None` when the key or value does not exist.
 * WHY:   RegGetValueW with RRF_RT_REG_SZ null-terminates and type-checks in one call. A value too long for the
 *        buffer (ERROR_MORE_DATA) cannot be `Deny`, so it is read as not denied without a second call.
 * WHERE: Win32PrivacyConsent::microphone.
 */
fn read_switch(hive: HKEY, subkey: &str) -> PortResult<Option<PermissionState>> {
    let mut buffer = [0u16; VALUE_CAPACITY];
    let mut bytes = u32::try_from(size_of_val(&buffer)).unwrap_or(u32::MAX);
    // SAFETY: `buffer` outlives the call and `bytes` holds its exact size in bytes, so RegGetValueW writes at most
    // that many bytes into it; the key and value names are valid null-terminated HSTRINGs for the whole call.
    let status = unsafe {
        RegGetValueW(
            hive,
            &HSTRING::from(subkey),
            &HSTRING::from(CONSENT_VALUE),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&raw mut bytes),
        )
    };
    match status {
        ERROR_SUCCESS => {
            let length = buffer
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(buffer.len());
            let value = String::from_utf16_lossy(&buffer[..length]);
            Ok(Some(if value == DENY {
                PermissionState::Denied
            } else {
                PermissionState::Granted
            }))
        }
        ERROR_MORE_DATA => Ok(Some(PermissionState::Granted)),
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
        other => Err(PortError::new(AppError::AudioDevice).with_detail(format!(
            "microphone consent read failed for {subkey}: Win32 error {}",
            other.0
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure() -> PortResult<Option<PermissionState>> {
        Err(PortError::new(AppError::AudioDevice).with_detail("access denied"))
    }

    #[test]
    fn any_deny_blocks_and_unset_switches_allow() {
        assert_eq!(combine([]), Ok(PermissionState::Granted));
        assert_eq!(
            combine([Ok(None), Ok(Some(PermissionState::Granted))]),
            Ok(PermissionState::Granted)
        );
        assert_eq!(
            combine([
                Ok(Some(PermissionState::Granted)),
                Ok(Some(PermissionState::Denied))
            ]),
            Ok(PermissionState::Denied)
        );
    }

    #[test]
    fn a_deny_outranks_a_failed_read_and_a_failure_is_never_guessed() {
        assert_eq!(
            combine([failure(), Ok(Some(PermissionState::Denied))]),
            Ok(PermissionState::Denied)
        );
        let failed = combine([Ok(None), failure()]).unwrap_err();
        assert_eq!(failed.error(), &AppError::AudioDevice);
        assert_eq!(failed.detail(), Some("access denied"));
    }

    #[test]
    fn user_switches_cover_user_desktop_apps_and_this_exe() {
        let consent = Win32PrivacyConsent::new();
        let keys: Vec<&str> = consent.user_switches.iter().map(String::as_str).collect();
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0], MICROPHONE_STORE);
        assert_eq!(keys[1], format!(r"{MICROPHONE_STORE}\NonPackaged"));
        let app_key = keys[2]
            .strip_prefix(keys[1])
            .and_then(|rest| rest.strip_prefix('\\'));
        assert!(app_key.is_some_and(|key| key.ends_with(".exe") && !key.contains('\\')));
    }

    /// Reads the real consent store of the machine running the tests: whatever the switches say, it answers.
    #[test]
    fn reads_this_machines_consent_store() {
        assert!(Win32PrivacyConsent::new().microphone().is_ok());
        assert_eq!(
            read_switch(HKEY_CURRENT_USER, r"SOFTWARE\Echo\NoSuchKey"),
            Ok(None)
        );
    }
}
