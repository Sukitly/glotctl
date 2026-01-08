//! Missing translation key checker.
//!
//! This module analyzes TSX/JSX files to find translation keys that are used
//! in code but missing from the locale JSON files. It handles:
//!
//! - Static keys: `t("key.name")`
//! - Dynamic keys: `t(variable)` or `t(\`prefix.${id}\`)`
//! - Namespace extraction: `const t = useTranslations("namespace")`
//! - Schema-based translations: factory patterns for complex key generation
//!
//! The checker uses the `ValueAnalyzer` to resolve dynamic expressions and
//! track variable bindings across the codebase.

mod checker;
mod types;

pub use checker::MissingKeyChecker;
pub use types::{DynamicKeyReason, MissingKeyResult, UsedKey};

#[cfg(test)]
mod tests;
