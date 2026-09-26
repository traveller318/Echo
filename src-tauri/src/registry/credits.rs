/*!
 * SOURCE OF TRUTH KEYWORDS: credits registry, CREDITS, third-party licenses, bundled components, ONNX Runtime license, DirectML license, Visual C++ runtime, Silero VAD license, Inter, Poppins, OFL
 * WHAT:  `CREDITS`: every third-party component the installer carries (runtime DLLs, the bundled Silero model, the
 *        typefaces), each with its license, credit line, shipped license texts and the resource files it covers.
 * WHY:   05 A15: About lists whose work Echo ships and under which terms. Downloadable models and runtimes are
 *        credited by their ModelManifest (registry/models.rs); this list covers what is inside the installer, so
 *        About shows each component once. The tests read tauri.conf.json's bundle map and fail when a resource file
 *        has no credit, a credit names a file that is not installed, or a license text ships unreferenced: adding
 *        a bundled binary is an entry here plus its license file, never a silent drop-in.
 * WHERE: ipc/commands/settings.rs (`registry_get` → RegistryView.credits) → Settings → About → Licenses
 *        (src/routes/settings/_components/about-copy.ts).
 */

use crate::types::{StaticList, StaticStr, ThirdPartyCredit};

const ONNXRUNTIME_LICENSES: &[StaticStr] = &[
    StaticStr::new("onnxruntime-LICENSE.txt"),
    StaticStr::new("onnxruntime-ThirdPartyNotices.txt"),
];
const ONNXRUNTIME_FILES: &[StaticStr] = &[StaticStr::new("onnxruntime/onnxruntime.dll")];
const DIRECTML_LICENSES: &[StaticStr] = &[
    StaticStr::new("DirectML-LICENSE.txt"),
    StaticStr::new("DirectML-ThirdPartyNotices.txt"),
];
const DIRECTML_FILES: &[StaticStr] = &[StaticStr::new("onnxruntime/DirectML.dll")];
const MSVC_RUNTIME_LICENSES: &[StaticStr] = &[StaticStr::new("msvc-runtime-NOTICE.txt")];
const MSVC_RUNTIME_FILES: &[StaticStr] = &[
    StaticStr::new("onnxruntime/msvcp140.dll"),
    StaticStr::new("onnxruntime/msvcp140_1.dll"),
    StaticStr::new("onnxruntime/vcruntime140.dll"),
    StaticStr::new("onnxruntime/vcruntime140_1.dll"),
];
const SILERO_VAD_V5_LICENSES: &[StaticStr] = &[StaticStr::new("silero-vad-LICENSE.txt")];
const SILERO_VAD_V5_FILES: &[StaticStr] = &[StaticStr::new("models/silero_vad.onnx")];
const INTER_LICENSES: &[StaticStr] = &[StaticStr::new("Inter-OFL.txt")];
const POPPINS_LICENSES: &[StaticStr] = &[StaticStr::new("Poppins-OFL.txt")];
/// The typefaces are compiled into the web assets, not installed as resource files.
const IN_WEB_ASSETS: &[StaticStr] = &[];

/// Every third-party component the installer ships, in the order About lists them.
pub const CREDITS: &[ThirdPartyCredit] = &[
    ThirdPartyCredit {
        id: StaticStr::new("onnxruntime"),
        label: StaticStr::new("ONNX Runtime 1.24.2"),
        license: StaticStr::new("MIT"),
        attribution: StaticStr::new("Copyright (c) Microsoft Corporation"),
        license_files: StaticList::new(ONNXRUNTIME_LICENSES),
        bundled_files: StaticList::new(ONNXRUNTIME_FILES),
    },
    ThirdPartyCredit {
        id: StaticStr::new("directml"),
        label: StaticStr::new("DirectML 1.15.4"),
        license: StaticStr::new("Microsoft Software License Terms"),
        attribution: StaticStr::new("Copyright (c) Microsoft Corporation"),
        license_files: StaticList::new(DIRECTML_LICENSES),
        bundled_files: StaticList::new(DIRECTML_FILES),
    },
    ThirdPartyCredit {
        id: StaticStr::new("msvc-runtime"),
        label: StaticStr::new("Microsoft Visual C++ Runtime 14.51"),
        license: StaticStr::new("Visual Studio Distributable Code"),
        attribution: StaticStr::new("Copyright (c) Microsoft Corporation"),
        license_files: StaticList::new(MSVC_RUNTIME_LICENSES),
        bundled_files: StaticList::new(MSVC_RUNTIME_FILES),
    },
    ThirdPartyCredit {
        id: StaticStr::new("silero-vad-v5"),
        label: StaticStr::new("Silero VAD v5"),
        license: StaticStr::new("MIT"),
        attribution: StaticStr::new("Copyright (c) 2020-present Silero Team"),
        license_files: StaticList::new(SILERO_VAD_V5_LICENSES),
        bundled_files: StaticList::new(SILERO_VAD_V5_FILES),
    },
    ThirdPartyCredit {
        id: StaticStr::new("inter"),
        label: StaticStr::new("Inter typeface"),
        license: StaticStr::new("OFL-1.1"),
        attribution: StaticStr::new("Copyright (c) 2016 The Inter Project Authors"),
        license_files: StaticList::new(INTER_LICENSES),
        bundled_files: StaticList::new(IN_WEB_ASSETS),
    },
    ThirdPartyCredit {
        id: StaticStr::new("poppins"),
        label: StaticStr::new("Poppins typeface"),
        license: StaticStr::new("OFL-1.1"),
        attribution: StaticStr::new("Copyright 2020 The Poppins Project Authors"),
        license_files: StaticList::new(POPPINS_LICENSES),
        bundled_files: StaticList::new(IN_WEB_ASSETS),
    },
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{
        registry::{models::MODELS, tests::is_registry_id},
        types::testing::installed_resource_files,
    };

    const LICENSES: &str = "licenses/";

    fn named(files: &StaticList<StaticStr>) -> impl Iterator<Item = &str> {
        files.iter().map(StaticStr::as_str)
    }

    #[test]
    fn ids_are_unique_kebab_case_and_every_credit_ships_a_license_text() {
        let ids: BTreeSet<&str> = CREDITS.iter().map(|credit| credit.id.as_str()).collect();
        assert_eq!(ids.len(), CREDITS.len());
        for credit in CREDITS {
            assert!(is_registry_id(credit.id.as_str()), "{}", credit.id.as_str());
            assert!(!credit.license_files.is_empty(), "{}", credit.id.as_str());
        }
    }

    #[test]
    fn every_installed_resource_is_credited_once_and_every_credited_file_is_installed() {
        let installed = installed_resource_files();
        let mut credited = BTreeSet::new();
        for credit in CREDITS {
            for file in named(&credit.bundled_files) {
                assert!(
                    installed.contains(file),
                    "{file} is credited but not installed"
                );
                assert!(credited.insert(file.to_owned()), "{file} is credited twice");
            }
        }
        let shipped: BTreeSet<String> = installed
            .into_iter()
            .filter(|file| !file.starts_with(LICENSES))
            .collect();
        assert_eq!(shipped, credited, "every bundled file needs a credit");
    }

    #[test]
    fn every_license_text_is_installed_and_referenced() {
        let installed: BTreeSet<String> = installed_resource_files()
            .into_iter()
            .filter_map(|file| file.strip_prefix(LICENSES).map(str::to_owned))
            .collect();
        let referenced: BTreeSet<String> = CREDITS
            .iter()
            .flat_map(|credit| named(&credit.license_files))
            .map(str::to_owned)
            .collect();
        assert_eq!(installed, referenced);
    }

    #[test]
    fn bundled_models_are_credited_here_and_downloadable_ones_by_their_manifest() {
        let credited: BTreeSet<&str> = CREDITS
            .iter()
            .flat_map(|credit| named(&credit.bundled_files))
            .collect();
        for manifest in MODELS.iter().filter(|manifest| manifest.bundled) {
            for file in manifest.files.iter() {
                let path = format!("models/{}", file.name.as_str());
                assert!(credited.contains(path.as_str()), "{path} has no credit");
            }
        }
    }
}
