#[path = "udl/folding.rs"]
mod folding;
#[path = "udl/lexer.rs"]
mod lexer;
#[path = "udl/model.rs"]
mod model;
#[path = "udl/number.rs"]
mod number;
pub use lexer::{FoldLevel, FoldRange, LexResult, Span, lex};
pub use model::*;
