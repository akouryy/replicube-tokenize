#[derive(Debug, Clone)]
pub enum Warning {
    WhitespaceBetweenCommaAndBracket { pos: usize },
    Semicolon { pos: usize },
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Warning::WhitespaceBetweenCommaAndBracket { pos } => {
                write!(
                    f,
                    "byte {pos}: whitespace between `,` and `[` wastes a token; write `,[` instead"
                )
            }
            Warning::Semicolon { pos } => {
                write!(f, "byte {pos}: replace `;` with whitespace or a comma")
            }
        }
    }
}
