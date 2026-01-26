use crate::analysis::data::message::MessageLocation;
use crate::analysis::data::value_type::ValueType;

/// Information about a locale with mismatched value type.
///
/// Used in `TypeMismatchIssue` to describe which locales have
/// different types than the primary locale.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocaleTypeMismatch {
    pub locale: String,
    pub actual_type: ValueType,
    pub location: MessageLocation,
}

impl LocaleTypeMismatch {
    pub fn new(
        locale: impl Into<String>,
        actual_type: ValueType,
        location: MessageLocation,
    ) -> Self {
        Self {
            locale: locale.into(),
            actual_type,
            location,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis::{LocaleTypeMismatch, MessageLocation, ValueType};

    #[test]
    fn test_locale_type_mismatch_new() {
        let loc = MessageLocation::new("./messages/zh.json", 8, 1);
        let mismatch = LocaleTypeMismatch::new("zh", ValueType::String, loc);
        assert_eq!(mismatch.locale, "zh");
        assert_eq!(mismatch.actual_type, ValueType::String);
        assert_eq!(mismatch.location.file_path, "./messages/zh.json");
    }
}
