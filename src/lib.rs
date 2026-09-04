#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

pub mod ast;
pub(crate) mod emoji_strings;
pub mod interpreter;
pub mod lexer;
pub mod parser;
pub mod types;
pub(crate) mod unicode_tables;
