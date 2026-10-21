use tree_sitter::Node;

use super::javascript::{
    js_bindings, js_hints, js_import, js_locals, js_references, js_symbol, JS_SKIP,
};
use crate::extract::{Cx, LanguageSupport};
use crate::types::{
    IndexedBinding, IndexedImport, IndexedLocal, IndexedReference, IndexedSymbol, IndexedTypeHint,
    Language,
};

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

            fn reference(&self, _cx: &Cx<'_>, _node: Node<'_>) -> Option<IndexedReference> {
                None
            }

            fn references(&self, cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedReference>) {
                js_references(cx, node, out)
            }

            fn bindings(&self, cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedBinding>) {
                js_bindings(cx, node, out)
            }

            fn hints(&self, cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedTypeHint>) {
                js_hints(cx, node, out)
            }

            fn locals(&self, cx: &Cx<'_>, node: Node<'_>, out: &mut Vec<IndexedLocal>) {
                js_locals(cx, node, out)
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
