#![expect(clippy::missing_errors_doc)]

mod ast_impl;
mod diagnostics;
mod options;
mod parser;
mod surrogate_pair;

mod generated {
    #[cfg(debug_assertions)]
    pub mod assert_layouts;
    mod derive_clone_in;
    mod derive_content_eq;
    #[cfg(feature = "serialize")]
    mod derive_estree;
}

pub mod ast;
pub use crate::{
    ast_impl::visit,
    options::Options,
    parser::{ConstructorParser, LiteralParser},
};
