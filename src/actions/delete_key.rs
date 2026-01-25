//! DeleteKey action.
//!
//! Deletes unused or orphan translation keys from JSON files.
//! Used by the `glot clean` command.

use crate::types::{OrphanKeyIssue, UnusedKeyIssue};

use super::operation::Operation;
use super::traits::Action;

/// Action to delete translation keys from JSON files.
///
/// This action supports multiple Issue types:
/// - `UnusedKeyIssue`: keys defined but not used in code
/// - `OrphanKeyIssue`: keys in non-primary locale but not in primary
pub struct DeleteKey;

impl Action<UnusedKeyIssue> for DeleteKey {
    fn to_operations(issues: &[UnusedKeyIssue]) -> Vec<Operation> {
        issues
            .iter()
            .map(|issue| Operation::DeleteJsonKey {
                context: issue.context.clone(),
            })
            .collect()
    }
}

impl Action<OrphanKeyIssue> for DeleteKey {
    fn to_operations(issues: &[OrphanKeyIssue]) -> Vec<Operation> {
        issues
            .iter()
            .map(|issue| Operation::DeleteJsonKey {
                context: issue.context.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MessageContext, MessageLocation};

    #[test]
    fn test_unused_key_to_operations() {
        let loc = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx = MessageContext::new(loc, "Common.unused", "Unused value");
        let issue = UnusedKeyIssue { context: ctx };

        let ops = DeleteKey::to_operations(&[issue]);

        assert_eq!(ops.len(), 1);
        match &ops[0] {
            Operation::DeleteJsonKey { context } => {
                assert_eq!(context.file_path(), "./messages/en.json");
                assert_eq!(context.key, "Common.unused");
            }
            _ => panic!("Expected DeleteJsonKey"),
        }
    }

    #[test]
    fn test_orphan_key_to_operations() {
        let loc = MessageLocation::new("./messages/zh.json", 10, 3);
        let ctx = MessageContext::new(loc, "Common.orphan", "orphan value");
        let issue = OrphanKeyIssue {
            context: ctx,
            locale: "zh".to_string(),
        };

        let ops = DeleteKey::to_operations(&[issue]);

        assert_eq!(ops.len(), 1);
        match &ops[0] {
            Operation::DeleteJsonKey { context } => {
                assert_eq!(context.file_path(), "./messages/zh.json");
                assert_eq!(context.key, "Common.orphan");
            }
            _ => panic!("Expected DeleteJsonKey"),
        }
    }

    #[test]
    fn test_multiple_issues() {
        let loc1 = MessageLocation::new("./messages/en.json", 5, 3);
        let ctx1 = MessageContext::new(loc1, "Common.a", "A");
        let issue1 = UnusedKeyIssue { context: ctx1 };

        let loc2 = MessageLocation::new("./messages/en.json", 10, 3);
        let ctx2 = MessageContext::new(loc2, "Common.b", "B");
        let issue2 = UnusedKeyIssue { context: ctx2 };

        let ops = DeleteKey::to_operations(&[issue1, issue2]);

        assert_eq!(ops.len(), 2);
    }
}
