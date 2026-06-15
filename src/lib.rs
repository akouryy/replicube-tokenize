mod tokenizer;

pub use tokenizer::{Token, tokenize};

/// Total cost of the source, or `None` if any token has an undetermined cost.
pub fn count(src: &str) -> Option<usize> {
    tokenize(src).iter().map(|tok| tok.cost).sum()
}
