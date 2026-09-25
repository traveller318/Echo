/*!
 * SOURCE OF TRUTH KEYWORDS: Journal, WAV journal, hound writer, header flush, repair WAV header, read journal, remove journal, never lose a take
 * WHAT:  The on-disk journal of a take: `Journal` streams 16 kHz mono 16-bit PCM to `recordings/<id>.wav`,
 *        `repair` fixes the header of a journal left by a crash, `read` loads one back as 16 kHz f32 for retry, and
 *        `remove` deletes one (a discarded take).
 * WHY:   Audio is on disk while the user speaks, so a crash never loses a take (02 §7.3, 00 constraint 5). The
 *        header's sizes are rewritten every JOURNAL_FLUSH_SAMPLES (1 s), so even a killed process leaves a file that
 *        opens with all but the last second counted; `repair` then recounts the data from the file length (startup
 *        recovery). `finalize` writes the exact header on every normal stop. 16-bit PCM keeps a minute at about
 *        1.9 MB (05 decision log); `read` returns the same values the capture worker fed ASR, so a retry is
 *        deterministic (05 A2). A journal is a file under the resolved recordings folder, not a database row, so
 *        it lives in the pipeline next to the worker that writes it.
 * WHERE: Written by the capture worker (pipeline/capture/mod.rs); `repair` by startup recovery and retry, `read` by retry
 *        (pipeline/recovery.rs, pipeline/retry.rs); `remove` by the session when a take is discarded, History delete
 *        and the retention sweep (pipeline/retention.rs).
 */

use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};

use super::convert::from_pcm16;
use crate::types::{AppError, PIPELINE_SAMPLE_RATE_HZ, PortError, PortResult};

/// The journal format: what ASR consumes, stored as 16-bit PCM.
const JOURNAL_SPEC: WavSpec = WavSpec {
    channels: 1,
    sample_rate: PIPELINE_SAMPLE_RATE_HZ,
    bits_per_sample: 16,
    sample_format: SampleFormat::Int,
};

/// Samples between header rewrites: at most this much audio is uncounted after a crash (1 s).
pub const JOURNAL_FLUSH_SAMPLES: u64 = PIPELINE_SAMPLE_RATE_HZ as u64;

/// A journal being written.
pub struct Journal {
    writer: WavWriter<BufWriter<File>>,
    samples: u64,
    unflushed: u64,
}

impl Journal {
    /// Creates the journal at `path` (and its folder); an existing file is replaced.
    pub fn create(path: &Path) -> PortResult<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|error| storage("create the recordings folder", &error))?;
        }
        let writer = WavWriter::create(path, JOURNAL_SPEC)
            .map_err(|error| storage("create the journal", &error))?;
        Ok(Self {
            writer,
            samples: 0,
            unflushed: 0,
        })
    }

    /// Appends samples; rewrites the header after every JOURNAL_FLUSH_SAMPLES.
    pub fn append(&mut self, samples: &[i16]) -> PortResult<()> {
        for &sample in samples {
            self.writer
                .write_sample(sample)
                .map_err(|error| storage("write the journal", &error))?;
        }
        self.samples += samples.len() as u64;
        self.unflushed += samples.len() as u64;
        if self.unflushed >= JOURNAL_FLUSH_SAMPLES {
            self.writer
                .flush()
                .map_err(|error| storage("flush the journal", &error))?;
            self.unflushed = 0;
        }
        Ok(())
    }

    /// Samples written so far.
    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// Writes the final header and closes the file; returns the samples written.
    pub fn finalize(self) -> PortResult<u64> {
        let samples = self.samples;
        self.writer
            .finalize()
            .map_err(|error| storage("finalize the journal", &error))?;
        Ok(samples)
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: repair journal header, crash recovery WAV, RIFF size, data chunk size, recount samples
 * WHAT:  Rewrites the RIFF and `data` chunk sizes of the WAV at `path` from its real length (dropping a torn last
 *        sample) and returns the number of samples it holds.
 * WHY:   A process killed between header rewrites leaves sizes that count less than the file holds (or zero). The
 *        chunk list is walked instead of assuming a 44-byte header, so a header with extra chunks is still found.
 * WHERE: Startup recovery (02 §7.3 step 4) before a `recording`/`transcribing` row becomes `recoverable`; retry
 *        before every read (pipeline/retry.rs).
 */
pub fn repair(path: &Path) -> PortResult<u64> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| storage("open the journal for repair", &error))?;
    let length = file
        .metadata()
        .map_err(|error| storage("read the journal size", &error))?
        .len();
    let layout = locate_data(&mut file, length)?;
    let data_bytes = (length - layout.data_start) / layout.block_align * layout.block_align;
    let data_size = u32::try_from(data_bytes)
        .map_err(|_| not_a_journal("data is larger than a WAV can hold"))?;
    let riff_size = u32::try_from(layout.data_start - 8 + data_bytes)
        .map_err(|_| not_a_journal("file is larger than a WAV can hold"))?;
    write_u32_at(&mut file, 4, riff_size)?;
    write_u32_at(&mut file, layout.data_start - 4, data_size)?;
    file.set_len(layout.data_start + data_bytes)
        .map_err(|error| storage("trim the journal", &error))?;
    file.sync_all()
        .map_err(|error| storage("sync the repaired journal", &error))?;
    Ok(data_bytes / layout.block_align)
}

/// Loads a journal as 16 kHz mono f32, the values its take was transcribed from.
pub fn read(path: &Path) -> PortResult<Vec<f32>> {
    let reader = WavReader::open(path).map_err(|error| storage("open the journal", &error))?;
    if reader.spec() != JOURNAL_SPEC {
        return Err(not_a_journal("unexpected WAV format"));
    }
    reader
        .into_samples::<i16>()
        .map(|sample| {
            sample
                .map(from_pcm16)
                .map_err(|error| storage("read the journal", &error))
        })
        .collect()
}

/// Deletes the journal at `path`; a journal that is already gone is not an error.
pub fn remove(path: &Path) -> PortResult<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            Err(storage("delete the journal", &error))
        }
        _ => Ok(()),
    }
}

/// Where the sample data of a WAV starts and how many bytes one sample frame takes.
struct DataLayout {
    data_start: u64,
    block_align: u64,
}

/// Walks the RIFF chunks to the `fmt ` block alignment and the start of `data`.
fn locate_data(file: &mut File, length: u64) -> PortResult<DataLayout> {
    let mut header = [0_u8; 12];
    read_exact_at(file, 0, &mut header)?;
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Err(not_a_journal("missing RIFF/WAVE header"));
    }
    let mut position = 12_u64;
    let mut block_align = None;
    while position + 8 <= length {
        let mut chunk = [0_u8; 8];
        read_exact_at(file, position, &mut chunk)?;
        let size = u64::from(u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]));
        let body = position + 8;
        match &chunk[0..4] {
            b"fmt " => {
                let mut format = [0_u8; 14];
                read_exact_at(file, body, &mut format)?;
                block_align = Some(u64::from(u16::from_le_bytes([format[12], format[13]])));
            }
            b"data" => {
                return match block_align {
                    Some(align) if align > 0 => Ok(DataLayout {
                        data_start: body,
                        block_align: align,
                    }),
                    _ => Err(not_a_journal("`data` comes before a valid `fmt ` chunk")),
                };
            }
            _ => {}
        }
        // Chunks are padded to an even size.
        position = body + size + (size & 1);
    }
    Err(not_a_journal("no `data` chunk"))
}

fn read_exact_at(file: &mut File, offset: u64, buffer: &mut [u8]) -> PortResult<()> {
    file.seek(SeekFrom::Start(offset))
        .and_then(|_| file.read_exact(buffer))
        .map_err(|error| storage("read the journal header", &error))
}

fn write_u32_at(file: &mut File, offset: u64, value: u32) -> PortResult<()> {
    file.seek(SeekFrom::Start(offset))
        .and_then(|_| file.write_all(&value.to_le_bytes()))
        .map_err(|error| storage("write the journal header", &error))
}

fn storage(action: &str, error: &dyn std::fmt::Display) -> PortError {
    PortError::new(AppError::Storage).with_detail(format!("could not {action}: {error}"))
}

fn not_a_journal(reason: &str) -> PortError {
    PortError::new(AppError::Storage).with_detail(format!("not a journal: {reason}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::testing::TempDir;

    fn ramp(len: usize) -> Vec<i16> {
        (0..len).map(|index| (index % 2000) as i16 - 1000).collect()
    }

    fn header_count(path: &Path) -> u32 {
        WavReader::open(path).unwrap().len()
    }

    #[test]
    fn a_finalized_journal_holds_every_sample_in_the_pipeline_format() {
        let dir = TempDir::new("finalize");
        let path = dir.join("recordings").join("take.wav");
        let samples = ramp(24_000);
        let mut journal = Journal::create(&path).unwrap();
        for chunk in samples.chunks(333) {
            journal.append(chunk).unwrap();
        }
        assert_eq!(journal.finalize().unwrap(), 24_000);
        let reader = WavReader::open(&path).unwrap();
        assert_eq!(reader.spec(), JOURNAL_SPEC);
        let read_back = read(&path).unwrap();
        assert_eq!(read_back.len(), 24_000);
        assert_eq!(read_back[1], from_pcm16(samples[1]));
    }

    /// The process is killed mid-take: the writer is never finalized or dropped, and the file is read as-is.
    #[test]
    fn an_abrupt_stop_leaves_a_valid_header_covering_every_flushed_second() {
        let dir = TempDir::new("abrupt");
        let path = dir.join("take.wav");
        let mut journal = Journal::create(&path).unwrap();
        journal.append(&ramp(20_000)).unwrap();
        // 1.25 s written: the header was rewritten at the first full second.
        assert!(header_count(&path) >= JOURNAL_FLUSH_SAMPLES as u32);
        let samples = read(&path).unwrap();
        assert!(samples.len() as u64 >= JOURNAL_FLUSH_SAMPLES);
        assert_eq!(journal.samples(), 20_000);
        drop(journal);
        assert_eq!(header_count(&path), 20_000, "dropping finalizes");
    }

    #[test]
    fn repair_recounts_data_left_after_a_crash() {
        let dir = TempDir::new("repair");
        let path = dir.join("take.wav");
        Journal::create(&path).unwrap().finalize().unwrap();
        // Samples that reached the disk after the last header rewrite, plus half a torn sample.
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        for sample in ramp(5_000) {
            file.write_all(&sample.to_le_bytes()).unwrap();
        }
        file.write_all(&[7]).unwrap();
        drop(file);
        assert_eq!(header_count(&path), 0);

        assert_eq!(repair(&path).unwrap(), 5_000);
        assert_eq!(header_count(&path), 5_000);
        assert_eq!(read(&path).unwrap().len(), 5_000);
        assert_eq!(repair(&path).unwrap(), 5_000, "repair is idempotent");
    }

    #[test]
    fn repair_and_read_refuse_files_that_are_not_journals() {
        let dir = TempDir::new("invalid");
        let path = dir.join("junk.wav");
        fs::write(&path, b"not a wav file at all").unwrap();
        assert_eq!(
            repair(&path).err().map(PortError::into_app_error),
            Some(AppError::Storage)
        );
        assert!(read(&path).is_err());
        assert!(read(&dir.join("missing.wav")).is_err());
    }

    #[test]
    fn remove_deletes_and_tolerates_a_missing_journal() {
        let dir = TempDir::new("remove");
        let path = dir.join("take.wav");
        Journal::create(&path).unwrap().finalize().unwrap();
        remove(&path).unwrap();
        assert!(!path.exists());
        remove(&path).unwrap();
    }
}
