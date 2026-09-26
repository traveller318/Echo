/*!
 * SOURCE OF TRUTH KEYWORDS: ThirdPartyCredit, third-party credit, bundled component license, license file, attribution, About licenses, resources licenses
 * WHAT:  ThirdPartyCredit: one third-party component Echo ships inside its installer (a runtime DLL, a bundled model,
 *        a typeface) with its license, the credit line it requires, the license texts shipped in `licenses/` and
 *        the installed resource files it covers.
 * WHY:   Downloadable models credit themselves through their ModelManifest; what the installer carries has no
 *        manifest, yet its license must reach the user (05 A15) and its text must ship beside it. Declaring the
 *        covered files lets one registry test prove that nothing lands in `resources/` without a credit and a
 *        license text, so a new bundled binary cannot slip in uncredited.
 * WHERE: Entries in registry/credits.rs (`CREDITS`); sent to the UI inside RegistryView (`registry_get`) and listed
 *        in Settings → About → Licenses.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{StaticList, StaticStr};

/// One third-party component the installer ships, with the license it is used under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ThirdPartyCredit {
    /// Kebab-case registry id.
    pub id: StaticStr,
    /// Name and pinned version, as the user reads it.
    pub label: StaticStr,
    /// SPDX id where one exists (`MIT`, `OFL-1.1`), otherwise the name of the vendor's terms.
    pub license: StaticStr,
    /// The copyright or credit line the license asks to be shown.
    pub attribution: StaticStr,
    /// License texts shipped in the installed `licenses/` folder (file names).
    pub license_files: StaticList<StaticStr>,
    /// Installed resource files this credit covers, relative to the resources folder (`onnxruntime/…`).
    pub bundled_files: StaticList<StaticStr>,
}
