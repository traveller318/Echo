/*!
 * SOURCE OF TRUTH KEYWORDS: TranscriptSelector SQL, where clause builder, bulk filter, status IN, created_before, has_audio, cleared_at
 * WHAT:  Turns a TranscriptSelector into a SQL `WHERE` condition with positional parameters.
 * WHY:   The bulk select and the bulk delete must match exactly the same rows, so both build their condition here.
 *        Only fixed column names and `?` placeholders are ever written into the SQL; every value is bound, so no
 *        selector content can change the statement. `statuses: Some(vec![])` becomes a condition that matches
 *        nothing (never "any"), so an empty list cannot widen a delete.
 * WHERE: services/transcripts/{list::list, list::select, delete::delete_matching}.
 */

use rusqlite::types::Value;

use crate::types::TranscriptSelector;

/// A SQL condition over `transcripts` and the values bound to its placeholders, in order.
pub(super) fn condition(selector: &TranscriptSelector) -> (String, Vec<Value>) {
    let mut clauses = Vec::new();
    let mut params = Vec::new();
    if let Some(statuses) = &selector.statuses {
        if statuses.is_empty() {
            clauses.push("0".to_owned());
        } else {
            let placeholders = vec!["?"; statuses.len()].join(", ");
            clauses.push(format!("status IN ({placeholders})"));
            params.extend(
                statuses
                    .iter()
                    .map(|status| Value::Text(status.as_str().to_owned())),
            );
        }
    }
    if let Some(before) = selector.created_before {
        clauses.push("created_at < ?".to_owned());
        params.push(Value::Integer(before.as_millis()));
    }
    if let Some(has_audio) = selector.has_audio {
        clauses.push(
            if has_audio {
                "audio_path IS NOT NULL"
            } else {
                "audio_path IS NULL"
            }
            .to_owned(),
        );
    }
    if let Some(cleared) = selector.cleared {
        clauses.push(
            if cleared {
                "cleared_at IS NOT NULL"
            } else {
                "cleared_at IS NULL"
            }
            .to_owned(),
        );
    }
    let sql = if clauses.is_empty() {
        "1".to_owned()
    } else {
        clauses.join(" AND ")
    };
    (sql, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TranscriptStatus, UnixMs};

    #[test]
    fn every_set_condition_is_joined_with_and() {
        let (sql, params) = condition(&TranscriptSelector {
            statuses: Some(vec![
                TranscriptStatus::Recording,
                TranscriptStatus::Transcribing,
            ]),
            created_before: Some(UnixMs::from_millis(10)),
            has_audio: Some(true),
            cleared: Some(false),
        });
        assert_eq!(
            sql,
            "status IN (?, ?) AND created_at < ? AND audio_path IS NOT NULL AND cleared_at IS NULL"
        );
        assert_eq!(
            params,
            [
                Value::Text("recording".to_owned()),
                Value::Text("transcribing".to_owned()),
                Value::Integer(10),
            ]
        );
    }

    #[test]
    fn no_condition_matches_all_and_an_empty_status_list_matches_none() {
        assert_eq!(condition(&TranscriptSelector::default()).0, "1");
        let (sql, params) = condition(&TranscriptSelector {
            statuses: Some(Vec::new()),
            ..TranscriptSelector::default()
        });
        assert_eq!(sql, "0");
        assert!(params.is_empty());
        assert_eq!(
            condition(&TranscriptSelector {
                has_audio: Some(false),
                ..TranscriptSelector::default()
            })
            .0,
            "audio_path IS NULL"
        );
        assert_eq!(
            condition(&TranscriptSelector {
                cleared: Some(true),
                ..TranscriptSelector::default()
            })
            .0,
            "cleared_at IS NOT NULL"
        );
    }
}
