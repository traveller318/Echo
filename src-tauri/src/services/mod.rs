/*!
 * SOURCE OF TRUTH KEYWORDS: services layer, database access, SQLite, Db handle, one verb one table, persistence
 * WHAT:  Layer 3: the only code that touches the database. `Db` (db.rs) owns the connections and migrations;
 *        `transcripts` and `settings` hold one verb per file for their table; `calendar` answers local-date
 *        questions with the database's clock (no table), so day grouping and day bounds agree.
 * WHY:   Keeping SQL isolated with no business rules and no service-to-service calls makes every write
 *        auditable and every rule testable in pipeline/ or ipc/ instead (02 §3.2, §7). Callers pass a `&Db` and
 *        never see a connection or rusqlite type, so the storage engine could change without touching them.
 *        Every verb returns PortResult: `AppError::Storage` for a database failure, with the SQLite message as
 *        log-only detail.
 * WHERE: Called from pipeline/ and ipc/commands via `use crate::services::<table>`; may import types/ only.
 */

pub mod calendar;
mod db;
pub mod settings;
pub mod transcripts;

pub use db::Db;
