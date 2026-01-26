//! Action trait definition.
//!
//! Actions convert Issues into Operations. Each Action can be implemented
//! for multiple Issue types, providing type-safe handling.

use anyhow::Result;
use std::collections::{HashMap, HashSet};

use super::operation::Operation;

/// Statistics from running an action.
#[derive(Debug, Clone, Default)]
pub struct ActionStats {
    /// Number of issues processed.
    pub processed: usize,
    /// Number of issues skipped (e.g., missing required fields).
    pub skipped: usize,
    /// Number of changes actually applied to files.
    pub changes_applied: usize,
    /// Number of files modified.
    pub files_modified: usize,
}

impl std::ops::AddAssign for ActionStats {
    fn add_assign(&mut self, other: Self) {
        self.processed += other.processed;
        self.skipped += other.skipped;
        self.changes_applied += other.changes_applied;
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
        execute_operations(&ops)
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

pub(crate) fn execute_operations(ops: &[Operation]) -> Result<ActionStats> {
    let total = ops.len();
    let mut files_modified: HashSet<String> = HashSet::new();
    let mut changes_applied = 0;

    let mut insert_ops_by_file: HashMap<String, Vec<Operation>> = HashMap::new();
    let mut delete_ops: Vec<Operation> = Vec::new();

    for op in ops {
        match op {
            Operation::InsertComment { context, .. } => {
                insert_ops_by_file
                    .entry(context.file_path().to_string())
                    .or_default()
                    .push(op.clone());
            }
            Operation::DeleteJsonKey { .. } => {
                delete_ops.push(op.clone());
            }
        }
    }

    for (file_path, file_ops) in insert_ops_by_file {
        let applied = Operation::apply_insert_comment_ops(&file_ops)?;
        if applied > 0 {
            changes_applied += applied;
            files_modified.insert(file_path);
        }
    }

    for op in delete_ops {
        let result = op.execute()?;
        if result.is_applied() {
            changes_applied += 1;
            if let Operation::DeleteJsonKey { context } = op {
                files_modified.insert(context.file_path().to_string());
            }
        }
    }

    Ok(ActionStats {
        processed: total,
        skipped: 0,
        changes_applied,
        files_modified: files_modified.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_stats_add_assign() {
        let mut stats1 = ActionStats {
            processed: 5,
            skipped: 1,
            changes_applied: 4,
            files_modified: 2,
        };
        let stats2 = ActionStats {
            processed: 3,
            skipped: 2,
            changes_applied: 1,
            files_modified: 1,
        };

        stats1 += stats2;

        assert_eq!(stats1.processed, 8);
        assert_eq!(stats1.skipped, 3);
        assert_eq!(stats1.changes_applied, 5);
        assert_eq!(stats1.files_modified, 3);
    }

    #[test]
    fn test_action_stats_default() {
        let stats = ActionStats::default();
        assert_eq!(stats.processed, 0);
        assert_eq!(stats.skipped, 0);
        assert_eq!(stats.changes_applied, 0);
        assert_eq!(stats.files_modified, 0);
    }
}
