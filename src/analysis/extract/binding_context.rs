//! Translation function binding context management.
//!
//! Tracks translation function bindings (e.g., `const t = useTranslations("Namespace")`)
//! with proper scope handling for nested functions and arrow functions.

use std::collections::HashMap;

use crate::analysis::extract::translation_source::TranslationSource;

/// Manages translation function bindings with scope tracking.
///
/// This struct maintains a stack of scopes, where each scope contains
/// bindings for translation functions. When entering a function or
/// arrow function body, a new scope is pushed. When exiting, it's popped.
///
/// # Example
/// ```ignore
/// const t = useTranslations("Common");  // scope 0: t -> Direct("Common")
/// function inner() {                     // push scope 1
///     const t2 = useTranslations("Auth"); // scope 1: t2 -> Direct("Auth")
///     t("key");   // looks up from scope 1 -> scope 0, finds t in scope 0
///     t2("key");  // finds t2 in scope 1
/// }                                      // pop scope 1
/// ```
pub struct BindingContext {
    /// Stack of binding scopes (innermost last).
    bindings_stack: Vec<HashMap<String, TranslationSource>>,
}

impl Default for BindingContext {
    fn default() -> Self {
        Self::new()
    }
}

impl BindingContext {
    /// Create a new BindingContext with a single global scope.
    pub fn new() -> Self {
        Self {
            bindings_stack: vec![HashMap::new()],
        }
    }

    /// Enter a new scope (e.g., when entering a function body).
    pub fn enter_scope(&mut self) {
        self.bindings_stack.push(HashMap::new());
    }

    /// Exit the current scope (e.g., when leaving a function body).
    /// Keeps at least the global scope.
    pub fn exit_scope(&mut self) {
        if self.bindings_stack.len() > 1 {
            self.bindings_stack.pop();
        }
    }

    /// Insert a binding in the current (innermost) scope.
    pub fn insert_binding(&mut self, name: String, source: TranslationSource) {
        if let Some(scope) = self.bindings_stack.last_mut() {
            scope.insert(name, source);
        }
    }

    /// Look up a binding by name, searching from innermost to outermost scope.
    pub fn get_binding(&self, name: &str) -> Option<&TranslationSource> {
        for scope in self.bindings_stack.iter().rev() {
            if let Some(source) = scope.get(name) {
                return Some(source);
            }
        }
        None
    }

    /// Check if a name exists in the current (innermost) scope.
    pub fn is_in_current_scope(&self, name: &str) -> bool {
        self.bindings_stack
            .last()
            .is_some_and(|scope| scope.contains_key(name))
    }

    /// Check if a name has a binding in any outer scope (excluding current scope).
    /// Used for detecting shadowing.
    pub fn has_outer_binding(&self, name: &str) -> bool {
        self.bindings_stack
            .iter()
            .rev()
            .skip(1) // Skip current scope
            .any(|scope| scope.contains_key(name))
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis::extract::binding_context::*;

    #[test]
    fn test_new_has_global_scope() {
        let ctx = BindingContext::new();
        assert_eq!(ctx.bindings_stack.len(), 1);
    }

    #[test]
    fn test_enter_exit_scope() {
        let mut ctx = BindingContext::new();
        ctx.enter_scope();
        assert_eq!(ctx.bindings_stack.len(), 2);
        ctx.exit_scope();
        assert_eq!(ctx.bindings_stack.len(), 1);
    }

    #[test]
    fn test_exit_scope_keeps_global() {
        let mut ctx = BindingContext::new();
        ctx.exit_scope(); // Should not remove global scope
        assert_eq!(ctx.bindings_stack.len(), 1);
    }

    #[test]
    fn test_insert_and_get_binding() {
        let mut ctx = BindingContext::new();
        ctx.insert_binding(
            "t".to_string(),
            TranslationSource::Direct {
                namespace: Some("Common".to_string()),
            },
        );

        let binding = ctx.get_binding("t");
        assert!(binding.is_some());
        assert!(matches!(
            binding.unwrap(),
            TranslationSource::Direct { namespace } if namespace == &Some("Common".to_string())
        ));
    }

    #[test]
    fn test_inner_scope_shadows_outer() {
        let mut ctx = BindingContext::new();
        ctx.insert_binding(
            "t".to_string(),
            TranslationSource::Direct {
                namespace: Some("Outer".to_string()),
            },
        );

        ctx.enter_scope();
        ctx.insert_binding(
            "t".to_string(),
            TranslationSource::Direct {
                namespace: Some("Inner".to_string()),
            },
        );

        // Should find inner scope binding
        let binding = ctx.get_binding("t");
        assert!(matches!(
            binding.unwrap(),
            TranslationSource::Direct { namespace } if namespace == &Some("Inner".to_string())
        ));

        ctx.exit_scope();

        // Should find outer scope binding now
        let binding = ctx.get_binding("t");
        assert!(matches!(
            binding.unwrap(),
            TranslationSource::Direct { namespace } if namespace == &Some("Outer".to_string())
        ));
    }

    #[test]
    fn test_is_in_current_scope() {
        let mut ctx = BindingContext::new();
        ctx.insert_binding("t".to_string(), TranslationSource::Shadowed);

        assert!(ctx.is_in_current_scope("t"));
        assert!(!ctx.is_in_current_scope("other"));

        ctx.enter_scope();
        assert!(!ctx.is_in_current_scope("t")); // t is in outer scope
    }

    #[test]
    fn test_has_outer_binding() {
        let mut ctx = BindingContext::new();
        ctx.insert_binding("t".to_string(), TranslationSource::Shadowed);

        assert!(!ctx.has_outer_binding("t")); // Only in current scope

        ctx.enter_scope();
        assert!(ctx.has_outer_binding("t")); // Now t is in outer scope

        ctx.insert_binding("t".to_string(), TranslationSource::Shadowed);
        // Still has outer binding even though current scope also has t
        assert!(ctx.has_outer_binding("t"));
    }
}
