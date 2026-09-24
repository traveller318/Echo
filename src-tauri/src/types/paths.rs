/*!
 * SOURCE OF TRUTH KEYWORDS: AppPaths, app data layout, echo.db path, database backup path, recordings dir, recording name, models dir, runtimes dir, logs dir, resources dir
 * WHAT:  AppPaths: every location Echo reads or writes (02 §7.1), derived from two roots the composition root
 *        resolves: the per-user app data folder and the bundled resources folder.
 * WHY:   Paths are never hardcoded (05 W23) and never resolved below app/: bootstrap asks the Tauri path API for
 *        the two roots once and hands this value down. Every file and folder name under them is spelled here
 *        only, so the database, journal, model manager and log sink cannot drift apart. Model folders are
 *        `models/<id>/` with downloads staged in `models/<id>.partial/` (02 §8.2); journals are
 *        `recordings/<transcript id>.wav` (02 §7.3), and the row stores only that file name; the pre-migration
 *        copy is `echo.db.bak-<from_version>` (02 §7.2).
 * WHERE: Built by app/bootstrap; carried by registry::engines::BuildCtx; read by services/db (database file, backup),
 *        the model store, the capture journal, retention and the log sink.
 */

use std::path::{Path, PathBuf};

use super::{ModelId, TranscriptId};

/// Where Echo keeps its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    data_dir: PathBuf,
    resources_dir: PathBuf,
}

impl AppPaths {
    /// `data_dir` is the Tauri `app_local_data_dir()`; `resources_dir` is the bundled resources folder.
    pub fn new(data_dir: impl Into<PathBuf>, resources_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            resources_dir: resources_dir.into(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Read-only files shipped with the installer (Silero VAD, sounds, ONNX Runtime DLLs).
    pub fn resources_dir(&self) -> &Path {
        &self.resources_dir
    }

    /// The SQLite database.
    pub fn database(&self) -> PathBuf {
        self.data_dir.join("echo.db")
    }

    /// Where the database is copied before a migration from schema `from_version` (02 §7.2).
    pub fn database_backup(&self, from_version: usize) -> PathBuf {
        self.data_dir.join(format!("echo.db.bak-{from_version}"))
    }

    /// WAV journals of takes.
    pub fn recordings_dir(&self) -> PathBuf {
        self.data_dir.join("recordings")
    }

    /// The journal file name of one take, relative to `recordings_dir()`; stored as `transcripts.audio_path`.
    pub fn recording_name(id: TranscriptId) -> String {
        format!("{id}.wav")
    }

    /// The journal of one take.
    pub fn recording(&self, id: TranscriptId) -> PathBuf {
        self.recordings_dir().join(Self::recording_name(id))
    }

    /// Installed models, one folder per model id.
    pub fn models_dir(&self) -> PathBuf {
        self.data_dir.join("models")
    }

    /// The folder of an installed model.
    pub fn model_dir(&self, id: &ModelId) -> PathBuf {
        self.models_dir().join(id.as_str())
    }

    /// Where a download or import is staged until it is verified and renamed into place.
    pub fn model_partial_dir(&self, id: &ModelId) -> PathBuf {
        self.models_dir().join(format!("{id}.partial"))
    }

    /// Downloaded sidecar runtimes (llama-server).
    pub fn runtimes_dir(&self) -> PathBuf {
        self.data_dir.join("runtimes")
    }

    /// Rolling local log files, named `<LOG_FILE_PREFIX>.<date>.<LOG_FILE_SUFFIX>`.
    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    /// Start of every log file name in `logs_dir()`.
    pub const LOG_FILE_PREFIX: &str = "echo";

    /// Extension of every log file in `logs_dir()`.
    pub const LOG_FILE_SUFFIX: &str = "log";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_location_sits_under_its_root() {
        let data = PathBuf::from("data");
        let paths = AppPaths::new(&data, "resources");
        let model = ModelId::from_static("parakeet-tdt-0.6b-v3");
        let take: TranscriptId = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();

        assert_eq!(paths.database(), data.join("echo.db"));
        assert_eq!(paths.database_backup(3), data.join("echo.db.bak-3"));
        assert_eq!(
            AppPaths::recording_name(take),
            "01ARZ3NDEKTSV4RRFFQ69G5FAV.wav"
        );
        assert_eq!(
            paths.recording(take),
            data.join("recordings")
                .join("01ARZ3NDEKTSV4RRFFQ69G5FAV.wav")
        );
        assert_eq!(
            paths.model_dir(&model),
            data.join("models").join("parakeet-tdt-0.6b-v3")
        );
        assert_eq!(
            paths.model_partial_dir(&model),
            data.join("models").join("parakeet-tdt-0.6b-v3.partial")
        );
        assert_eq!(paths.runtimes_dir(), data.join("runtimes"));
        assert_eq!(paths.logs_dir(), data.join("logs"));
        assert_eq!(paths.resources_dir(), Path::new("resources"));
        assert_eq!(paths.data_dir(), data);
    }
}
