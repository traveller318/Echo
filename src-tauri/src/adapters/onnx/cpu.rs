/*!
 * SOURCE OF TRUTH KEYWORDS: cpu_cores, CpuCores, physical CPU cores, performance cores, hybrid CPU, EfficiencyClass, GetLogicalProcessorInformationEx, RelationProcessorCore
 * WHAT:  `cpu_cores`: how many physical processor cores this machine has and how many of them are performance
 *        cores (the highest efficiency class), read once from Windows and cached; falls back to the logical count
 *        when Windows cannot answer.
 * WHY:   05 A9 sizes the ASR session's intra-op pool from *physical* cores: two ONNX threads on one hyper-threaded
 *        core fight over the same execution units, and `std` only reports logical processors. Hybrid CPUs (Intel
 *        12th gen and later) mix performance and efficiency cores; ONNX Runtime splits each operator evenly across
 *        its threads, so a thread on a slow core holds every operator back (measured on the dev box's i5-13420H,
 *        4P + 4E: 4 threads ran the encoder faster than 7, 05 W35). GetLogicalProcessorInformationEx
 *        (RelationProcessorCore) returns one variable-length record per core, whose PROCESSOR_RELATIONSHIP holds the
 *        core's EfficiencyClass (higher is faster; every core is 0 on a non-hybrid CPU), so counting records and the
 *        records of the top class gives both numbers. The buffer is plain bytes, so every field is read by offset.
 *        Every Windows call stays in adapters/ (root CLAUDE.md §3).
 * WHERE: SessionThreads::for_asr (adapters/onnx/session.rs), used by the Parakeet adapter.
 */

use std::{num::NonZeroUsize, sync::OnceLock};

#[cfg(windows)]
use windows::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationProcessorCore,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};

/// Physical cores and how many of them are performance cores (all of them on a non-hybrid CPU).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuCores {
    pub physical: usize,
    pub performance: usize,
}

impl CpuCores {
    /// Performance and efficiency cores are mixed.
    pub const fn is_hybrid(self) -> bool {
        self.performance < self.physical
    }
}

static CPU_CORES: OnceLock<CpuCores> = OnceLock::new();

#[cfg(windows)]
/// Byte offset of the `Size` field in every record (after the 4-byte `Relationship`).
const SIZE_OFFSET: usize = 4;
#[cfg(windows)]
/// Byte offset of PROCESSOR_RELATIONSHIP.EfficiencyClass (record header 8 bytes, then the `Flags` byte).
const EFFICIENCY_CLASS_OFFSET: usize = 9;

/// This machine's cores, at least one of each count.
pub fn cpu_cores() -> CpuCores {
    *CPU_CORES.get_or_init(|| {
        read_cores().unwrap_or_else(|| {
            let logical = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
            tracing::warn!(
                logical,
                "Windows did not report physical cores; using the logical processor count"
            );
            CpuCores {
                physical: logical,
                performance: logical,
            }
        })
    })
}

/// Asks Windows for one record per physical core; None when either call fails.
#[cfg(windows)]
fn read_cores() -> Option<CpuCores> {
    let mut length = 0_u32;
    // SAFETY: a null buffer with a length of 0 only asks for the size the buffer needs; the call writes `length`.
    // It reports ERROR_INSUFFICIENT_BUFFER as an error by design, so only the length matters here.
    let _ = unsafe { GetLogicalProcessorInformationEx(RelationProcessorCore, None, &mut length) };
    if length == 0 {
        return None;
    }
    let mut buffer = vec![0_u8; usize::try_from(length).ok()?];
    // SAFETY: `buffer` is `length` writable bytes and outlives the call; Windows writes at most `length` bytes and
    // updates `length` to the bytes written.
    unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            Some(
                buffer
                    .as_mut_ptr()
                    .cast::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX>(),
            ),
            &mut length,
        )
    }
    .ok()?;
    let written = usize::try_from(length).ok()?.min(buffer.len());
    count_cores(&buffer[..written])
}

#[cfg(windows)]
/// Counts the variable-length core records in `bytes` and those of the highest efficiency class; None if a record
/// is malformed or there is none.
fn count_cores(bytes: &[u8]) -> Option<CpuCores> {
    let mut classes = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let size_bytes: [u8; 4] = bytes
            .get(offset + SIZE_OFFSET..offset + SIZE_OFFSET + 4)?
            .try_into()
            .ok()?;
        let size = usize::try_from(u32::from_ne_bytes(size_bytes)).ok()?;
        if size <= EFFICIENCY_CLASS_OFFSET || offset + size > bytes.len() {
            return None;
        }
        classes.push(bytes[offset + EFFICIENCY_CLASS_OFFSET]);
        offset += size;
    }
    let top = classes.iter().copied().max()?;
    Some(CpuCores {
        physical: classes.len(),
        performance: classes.iter().filter(|class| **class == top).count(),
    })
}

/// Linux: physical cores are the distinct (package, core) pairs in `/proc/cpuinfo`; on a hybrid Intel CPU the kernel
/// lists the performance cores' logical CPUs in `/sys/devices/cpu_core/cpus`.
#[cfg(target_os = "linux")]
fn read_cores() -> Option<CpuCores> {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    let performance_cpus = std::fs::read_to_string("/sys/devices/cpu_core/cpus")
        .ok()
        .map(|list| parse_cpu_list(&list));
    count_cores(&cpuinfo, performance_cpus.as_deref())
}

/// `0-3,8,10-11` as the CPU numbers it names.
#[cfg(target_os = "linux")]
fn parse_cpu_list(list: &str) -> Vec<u32> {
    list.trim()
        .split(',')
        .filter_map(|part| match part.split_once('-') {
            Some((from, to)) => Some(from.parse::<u32>().ok()?..=to.parse::<u32>().ok()?),
            None => part.parse::<u32>().ok().map(|cpu| cpu..=cpu),
        })
        .flatten()
        .collect()
}

/// Counts distinct (physical id, core id) pairs; a core is a performance core when any of its logical CPUs is listed
/// in `performance_cpus` (every core when the list is absent). None when `cpuinfo` names no core.
#[cfg(target_os = "linux")]
fn count_cores(cpuinfo: &str, performance_cpus: Option<&[u32]>) -> Option<CpuCores> {
    use std::collections::BTreeMap;
    // (package, core) → is a performance core
    let mut cores: BTreeMap<(String, String), bool> = BTreeMap::new();
    for block in cpuinfo.split("\n\n") {
        let field = |name: &str| {
            block.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                (key.trim() == name).then(|| value.trim().to_owned())
            })
        };
        let (Some(processor), Some(core)) = (field("processor"), field("core id")) else {
            continue;
        };
        let package = field("physical id").unwrap_or_default();
        let listed = match (performance_cpus, processor.parse::<u32>()) {
            (Some(list), Ok(cpu)) => list.contains(&cpu),
            _ => true,
        };
        *cores.entry((package, core)).or_insert(false) |= listed;
    }
    (!cores.is_empty()).then(|| CpuCores {
        physical: cores.len(),
        performance: cores.values().filter(|performance| **performance).count(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_machine_reports_consistent_core_counts() {
        let logical = std::thread::available_parallelism().unwrap().get();
        let cores = cpu_cores();
        assert!(
            (1..=logical).contains(&cores.physical),
            "{cores:?} of {logical}"
        );
        assert!(
            (1..=cores.physical).contains(&cores.performance),
            "{cores:?}"
        );
    }

    #[cfg(windows)]
    fn record(size: u32, efficiency_class: u8) -> Vec<u8> {
        // Never shorter than the fields written, so a record can also declare an impossible size.
        let mut bytes = vec![0_u8; (size as usize).max(EFFICIENCY_CLASS_OFFSET + 1)];
        bytes[SIZE_OFFSET..SIZE_OFFSET + 4].copy_from_slice(&size.to_ne_bytes());
        bytes[EFFICIENCY_CLASS_OFFSET] = efficiency_class;
        bytes
    }

    #[cfg(windows)]
    #[test]
    fn hybrid_cores_are_split_by_efficiency_class() {
        let hybrid = [
            record(48, 1),
            record(48, 1),
            record(80, 0),
            record(48, 0),
            record(48, 0),
        ]
        .concat();
        let cores = count_cores(&hybrid).unwrap();
        assert_eq!(
            cores,
            CpuCores {
                physical: 5,
                performance: 2
            }
        );
        assert!(cores.is_hybrid());
        let uniform = [record(48, 0), record(48, 0)].concat();
        assert!(!count_cores(&uniform).unwrap().is_hybrid());
    }

    #[cfg(windows)]
    #[test]
    fn malformed_records_are_refused() {
        let bytes = [record(48, 0), record(48, 0)].concat();
        assert_eq!(count_cores(&[]), None);
        assert_eq!(
            count_cores(&record(8, 0)),
            None,
            "a record too small for its fields"
        );
        assert_eq!(count_cores(&bytes[..60]), None, "a truncated record");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cpuinfo_cores_are_counted_and_hybrid_ones_split() {
        let info = "processor : 0\nphysical id : 0\ncore id : 0\n\nprocessor : 1\nphysical id : 0\ncore id : 0\n\nprocessor : 2\nphysical id : 0\ncore id : 4\n\n";
        let uniform = count_cores(info, None).unwrap();
        assert_eq!(
            uniform,
            CpuCores {
                physical: 2,
                performance: 2
            }
        );
        let hybrid = count_cores(info, Some(&parse_cpu_list("0-1"))).unwrap();
        assert_eq!(
            hybrid,
            CpuCores {
                physical: 2,
                performance: 1
            }
        );
        assert!(hybrid.is_hybrid());
        assert_eq!(count_cores("model name : x\n", None), None);
        assert_eq!(parse_cpu_list("0-2,5\n"), [0, 1, 2, 5]);
    }
}
