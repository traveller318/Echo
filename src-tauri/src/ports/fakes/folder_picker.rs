/*!
 * SOURCE OF TRUTH KEYWORDS: FakeFolderPicker, fake folder dialog, picked folder test, cancelled picker
 * WHAT:  FakeFolderPicker: a FolderPicker that answers with a folder the test chose (or None, the user closed it),
 *        or fails once, and records every title it was shown with.
 * WHY:   Model import tests need a chosen folder (or a closed picker) without a dialog opening on the test machine.
 * WHERE: ipc::testing (default CommandCtx: no folder chosen); pipeline/models and ipc/commands/models tests.
 */

use std::{future, path::PathBuf, sync::Mutex};

use super::lock;
use crate::{
    ports::FolderPicker,
    types::{BoxFuture, PortError, PortResult},
};

#[derive(Default)]
struct PickerState {
    answer: Option<PathBuf>,
    next_error: Option<PortError>,
    titles: Vec<String>,
}

/// A folder picker whose answer the test sets.
#[derive(Default)]
pub struct FakeFolderPicker {
    state: Mutex<PickerState>,
}

impl FakeFolderPicker {
    /// Every later pick returns `folder` (None: the user closes the picker).
    pub fn answer(&self, folder: Option<PathBuf>) {
        lock(&self.state).answer = folder;
    }

    /// The next pick fails with `error`.
    pub fn fail_next(&self, error: PortError) {
        lock(&self.state).next_error = Some(error);
    }

    /// The title of every picker shown so far.
    pub fn titles(&self) -> Vec<String> {
        lock(&self.state).titles.clone()
    }
}

impl FolderPicker for FakeFolderPicker {
    fn pick_folder<'a>(&'a self, title: &'a str) -> BoxFuture<'a, PortResult<Option<PathBuf>>> {
        let mut state = lock(&self.state);
        let result = match state.next_error.take() {
            Some(error) => Err(error),
            None => {
                state.titles.push(title.to_owned());
                Ok(state.answer.clone())
            }
        };
        Box::pin(future::ready(result))
    }
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::{ports::fakes::poll_once, types::AppError};

    #[test]
    fn answers_what_the_test_chose_and_records_titles() {
        let picker = FakeFolderPicker::default();
        assert!(matches!(
            poll_once(picker.pick_folder("Pick")),
            Poll::Ready(Ok(None))
        ));
        picker.answer(Some(PathBuf::from("models")));
        assert!(matches!(
            poll_once(picker.pick_folder("Again")),
            Poll::Ready(Ok(Some(path))) if path.as_path() == std::path::Path::new("models")
        ));
        picker.fail_next(AppError::Internal.into());
        assert!(matches!(
            poll_once(picker.pick_folder("Fails")),
            Poll::Ready(Err(_))
        ));
        assert_eq!(picker.titles(), ["Pick", "Again"]);
    }
}
