use std::fmt;

/// Type of JSON value at a key path.
///
/// Used to detect type mismatches between primary and replica locales.
/// For example, if primary has an array but replica has a string, this
/// causes runtime crashes when the app tries to iterate over the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValueType {
    /// A simple string value
    String,
    /// A string array (accessed via t.raw() as a whole)
    StringArray,
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValueType::String => write!(f, "string"),
            ValueType::StringArray => write!(f, "array"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis::ValueType;

    #[test]
    fn test_value_type_display() {
        assert_eq!(ValueType::String.to_string(), "string");
        assert_eq!(ValueType::StringArray.to_string(), "array");
    }
}
