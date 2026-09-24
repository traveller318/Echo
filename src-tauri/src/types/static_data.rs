/*!
 * SOURCE OF TRUTH KEYWORDS: StaticStr, StaticList, Cow static, const registry data, borrowed or owned, specta string export
 * WHAT:  `StaticStr` (text) and `StaticList<T>` (a list) that hold either a `'static` borrow or an owned value.
 *        They serialize and export exactly like `string` and `T[]`.
 * WHY:   Registry entries and caps are `const` (borrowed literals and slices), while the same types must also
 *        deserialize from IPC and be built at runtime (owned). `Cow<'static, …>` gives that, but specta rc.25
 *        cannot export a `Cow` field (it reports an infinitely recursive inline type), so these wrappers own
 *        the `Cow` and describe themselves to specta as the plain string/list types.
 * WHERE: Ids (types/ids.rs), caps (types/engine.rs), setting specs and values (types/settings.rs).
 */

use std::{borrow::Cow, fmt, ops::Deref};

use serde::{Deserialize, Serialize};
use specta::{Type, Types, datatype::DataType};

/// Text that is either a `'static` literal or an owned string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StaticStr(Cow<'static, str>);

impl StaticStr {
    pub const fn new(text: &'static str) -> Self {
        Self(Cow::Borrowed(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for StaticStr {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl From<String> for StaticStr {
    fn from(text: String) -> Self {
        Self(Cow::Owned(text))
    }
}

impl fmt::Display for StaticStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Type for StaticStr {
    fn definition(types: &mut Types) -> DataType {
        String::definition(types)
    }
}

/// A list that is either a `'static` slice or an owned vector.
///
/// In a `const` entry, declare the slice as its own `const` item (`const LANGS: &[Language] = &[…];`) and pass
/// that: a temporary array of a type with a destructor is not promoted to `'static` through a call argument.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StaticList<T: Clone + 'static>(Cow<'static, [T]>);

impl<T: Clone + 'static> StaticList<T> {
    pub const fn new(items: &'static [T]) -> Self {
        Self(Cow::Borrowed(items))
    }
}

impl<T: Clone + 'static> Deref for StaticList<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.0
    }
}

impl<T: Clone + 'static> From<Vec<T>> for StaticList<T> {
    fn from(items: Vec<T>) -> Self {
        Self(Cow::Owned(items))
    }
}

impl<T: Clone + Type + 'static> Type for StaticList<T> {
    fn definition(types: &mut Types) -> DataType {
        Vec::<T>::definition(types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_and_owned_values_are_equal_and_serialize_plainly() {
        const TEXT: StaticStr = StaticStr::new("echo");
        assert_eq!(TEXT, StaticStr::from(String::from("echo")));
        assert_eq!(serde_json::to_string(&TEXT).unwrap(), "\"echo\"");
        assert_eq!(serde_json::from_str::<StaticStr>("\"echo\"").unwrap(), TEXT);

        const LIST: StaticList<u32> = StaticList::new(&[16_000, 48_000]);
        assert_eq!(LIST, StaticList::from(vec![16_000, 48_000]));
        assert!(LIST.contains(&48_000));
        assert_eq!(serde_json::to_string(&LIST).unwrap(), "[16000,48000]");
        assert_eq!(
            serde_json::from_str::<StaticList<u32>>("[1]")
                .unwrap()
                .len(),
            1
        );
    }
}
