use crate::extract::LanguageSupport;
use crate::types::Language;

mod c;
mod cpp;
mod csharp;
mod go;
mod java;
mod javascript;
mod php;
mod python;
mod ruby;
mod rust;
mod typescript;

pub(super) fn support_for(language: Language) -> &'static dyn LanguageSupport {
    match language {
        Language::JavaScript => &javascript::JavaScript,
        Language::TypeScript => &typescript::TypeScript,
        Language::Tsx => &typescript::Tsx,
        Language::Python => &python::Python,
        Language::Go => &go::Go,
        Language::Rust => &rust::Rust,
        Language::Java => &java::Java,
        Language::C => &c::C,
        Language::Cpp => &cpp::Cpp,
        Language::CSharp => &csharp::CSharp,
        Language::Ruby => &ruby::Ruby,
        Language::Php => &php::Php,
    }
}
