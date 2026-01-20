//! Rule implementations for glot.
//!
//! This module contains the concrete checker implementations that detect
//! various i18n issues in the codebase.

pub mod hardcoded;
pub mod missing;
pub mod orphan;
pub mod type_mismatch;
pub mod untranslated;

use anyhow::Result;

use crate::{commands::context::CheckContext, issue::Issue};

/// Trait for implementing a check rule.
///
/// Checkers are the core units of logic in glot. Each checker:
/// 1. Declares its dependencies (registries, messages) via `needs_*` methods.
/// 2. Implements the `check` method to inspect the code/project and return issues.
pub trait Checker {
    /// Unique identifier for the checker (e.g., "hardcoded", "missing-keys").
    fn name(&self) -> &str;

    /// Whether this checker needs registries (schemas, key objects) loaded.
    /// Default: false
    fn needs_registries(&self) -> bool {
        false
    }

    /// Whether this checker needs locale messages loaded.
    /// Default: false
    fn needs_messages(&self) -> bool {
        false
    }

    /// Execute the check logic using the provided context.
    ///
    /// # Arguments
    /// * `ctx` - The CheckContext containing configuration and cached data.
    ///
    /// # Returns
    /// A vector of found issues, or an error if the check failed to execute.
    fn check(&self, ctx: &CheckContext) -> Result<Vec<Issue>>;
}
