use tree_sitter::Node;

use super::javascript::{js_import, js_reference, js_symbol, JS_SKIP};
use crate::extract::{Cx, LanguageSupport};
use crate::types::{IndexedImport, IndexedReference, IndexedSymbol, Language};

pub struct TypeScript;
pub struct Tsx;

macro_rules! js_family {
    ($ty:ty, $language:expr, $grammar:expr) => {
        impl LanguageSupport for $ty {
            fn language(&self) -> Language {
                $language
            }

            fn grammar(&self) -> tree_sitter::Language {
                $grammar.into()
            }

            fn symbol(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedSymbol> {
                js_symbol(cx, node)
            }

            fn reference(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedReference> {
                js_reference(cx, node)
            }

            fn import(&self, cx: &Cx<'_>, node: Node<'_>) -> Option<IndexedImport> {
                js_import(cx, node)
            }

            fn skip_ref_names(&self) -> &'static [&'static str] {
                JS_SKIP
            }
        }
    };
}

js_family!(
    TypeScript,
    Language::TypeScript,
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT
);
js_family!(Tsx, Language::Tsx, tree_sitter_typescript::LANGUAGE_TSX);
