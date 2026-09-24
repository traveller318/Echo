/*!
 * SOURCE OF TRUTH KEYWORDS: PortError, PortResult, port failure, adapter error, internal detail, user-safe error, log detail
 * WHAT:  PortError, the error every port method returns: a user-safe AppError plus optional internal detail for
 *        the local log. PortResult<T> is the matching Result alias.
 * WHY:   Ports sit below the command factory, which maps internal errors into AppError and logs the detail
 *        (02 §4.1 step 6). An adapter knows the precise cause (a cpal message, an HRESULT, an HTTP status) but that
 *        text must never reach the UI, so it rides in `detail` next to the safe category. Display prints only the
 *        safe message; the detail is visible through `detail()` and Debug. There is deliberately no
 *        `From<PortError> for AppError`: turning one into the other goes through `into_app_error`, so dropping
 *        the detail is always a visible choice at the call site. Detail never contains transcript text or audio
 *        (02 §10).
 * WHERE: Returned by every trait in ports/ and by command handlers (any `E: Into<PortError>`); built by adapters/
 *        and the ports/fakes test doubles; unwrapped by pipeline/ and ipc/ (the factory logs `detail`, returns
 *        `error`).
 */

use thiserror::Error;

use super::AppError;

/// A failed port call: what the user may see (`error`) and what only the log may see (`detail`).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{error}")]
pub struct PortError {
    error: AppError,
    detail: Option<String>,
}

/// Result of a port method.
pub type PortResult<T> = Result<T, PortError>;

impl PortError {
    pub const fn new(error: AppError) -> Self {
        Self {
            error,
            detail: None,
        }
    }

    /// Attaches the internal cause for the local log (never shown to the user, never transcript text).
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// The user-safe error.
    pub const fn error(&self) -> &AppError {
        &self.error
    }

    /// The internal cause, if the adapter recorded one.
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }

    /// The user-safe error, discarding the detail; log `detail()` first when it matters.
    pub fn into_app_error(self) -> AppError {
        self.error
    }
}

impl From<AppError> for PortError {
    fn from(error: AppError) -> Self {
        Self::new(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_shows_only_the_safe_message() {
        let error = PortError::new(AppError::AudioDevice)
            .with_detail("cpal: The requested device is no longer available (0x88890004)");
        assert_eq!(error.to_string(), AppError::AudioDevice.to_string());
        assert_eq!(
            error.detail(),
            Some("cpal: The requested device is no longer available (0x88890004)")
        );
        assert_eq!(error.error(), &AppError::AudioDevice);
        assert_eq!(error.into_app_error(), AppError::AudioDevice);
    }

    #[test]
    fn app_errors_convert_without_detail() {
        let error = PortError::from(AppError::Busy);
        assert_eq!(error.detail(), None);
        assert_eq!(error, PortError::new(AppError::Busy));
    }
}
