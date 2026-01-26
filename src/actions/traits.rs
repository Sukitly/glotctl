//! Action trait definition.
//!
//! Actions convert Issues into Operations. Each Action can be implemented
//! for multiple Issue types, providing type-safe handling.

use anyhow::Result;

use crate::actions::operation::Operation;

/// Statistics from running an action.
#[derive(Debug, Clone, Default)]
pub struct ActionStats {
    /// Number of issues processed.
    pub processed: usize,
    /// Number of issues skipped (e.g., missing required fields).
    pub skipped: usize,
    /// Number of files modified.
    pub files_modified: usize,
}

impl std::ops::AddAssign for ActionStats {
    fn add_assign(&mut self, other: Self) {
        self.processed += other.processed;
        self.skipped += other.skipped;
        self.files_modified += other.files_modified;
    }
}

/// Action trait - converts Issues into Operations and executes them.
///
/// Each Action struct can implement this trait for multiple Issue types,
/// providing type-safe handling of different issues.
///
/// # Example
///
/// ```ignore
/// // In command code:
/// if apply {
///     InsertDisableComment::run(&issues)?;
/// } else {
///     InsertDisableComment::preview(&issues);
/// }
/// ```
pub trait Action<I> {
    /// Convert issues to low-level operations.
    ///
    /// This is the only method that must be implemented.
    fn to_operations(issues: &[I]) -> Vec<Operation>;

    /// Execute the action (modify files).
    ///
    /// Default implementation calls `to_operations` and executes each operation.
    fn run(issues: &[I]) -> Result<ActionStats> {
        let ops = Self::to_operations(issues);
        let total = ops.len();

        // Track unique files modified
        let mut files_modified = std::collections::HashSet::new();

        for op in &ops {
            op.execute()?;

            // Track file path
            match op {
                Operation::InsertComment { context, .. } => {
                    files_modified.insert(context.file_path().to_string());
                }
                Operation::DeleteJsonKey { context } => {
                    files_modified.insert(context.file_path().to_string());
                }
            }
        }

        Ok(ActionStats {
            processed: total,
            skipped: 0,
            files_modified: files_modified.len(),
        })
    }

    /// Preview the action (dry-run mode).
    ///
    /// Default implementation calls `to_operations` and previews each operation.
    fn preview(issues: &[I]) {
        let ops = Self::to_operations(issues);
        for op in &ops {
            op.preview();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::actions::traits::*;

    #[test]
    fn test_action_stats_add_assign() {
        let mut stats1 = ActionStats {
            processed: 5,
            skipped: 1,
            files_modified: 2,
        };
        let stats2 = ActionStats {
            processed: 3,
            skipped: 2,
            files_modified: 1,
        };

        stats1 += stats2;

        assert_eq!(stats1.processed, 8);
        assert_eq!(stats1.skipped, 3);
        assert_eq!(stats1.files_modified, 3);
    }

    #[test]
    fn test_action_stats_default() {
        let stats = ActionStats::default();
        assert_eq!(stats.processed, 0);
        assert_eq!(stats.skipped, 0);
        assert_eq!(stats.files_modified, 0);
    }
}
