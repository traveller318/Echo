/*!
 * SOURCE OF TRUTH KEYWORDS: services layer, database access, SQLite, one verb one table, persistence
 * WHAT:  Layer 3: the only code that touches the database, one verb per table per file.
 * WHY:   Keeping SQL isolated with no business rules and no service-to-service calls makes every write
 *        auditable and every rule testable in pipeline/ or ipc/ instead (02 §3.2, §7).
 * WHERE: Called from pipeline/ and ipc/commands via `use crate::services::<table>`; may import types/ only.
 */
