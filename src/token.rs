#[derive(Debug, Clone)]
pub struct Token<'a> {
    pub text: &'a str,
    pub(crate) is_after_assignment_lhs_comma: bool,
    pub kind: TokenKind<'a>,
}

#[derive(Debug, Clone)]
pub enum TokenKind<'a> {
    Str(&'a str),
    Number {
        is_hex: bool,
        int: &'a str,
        frac: Option<&'a str>,
        exp: Option<&'a str>,
    },
    Ident,
    ClosingBracket,
    Punct,
    OpenBrace,
    Unknown,
}

impl Token<'_> {
    /// Token cost, or `None` when the cost is undetermined (unrecognized input).
    pub fn cost(&self) -> Option<usize> {
        match &self.kind {
            TokenKind::Unknown => None,
            // Escapes are counted as raw source bytes, not decoded.
            TokenKind::Str(content) => Some(1usize << (content.len() / 2).min(15)),
            TokenKind::Number { is_hex: true, int, frac, exp } => {
                let e = match exp {
                    Some(s) => parse_value(s, 10).unwrap_or(u64::MAX),
                    None => 0,
                };
                match frac {
                    Some(f) => match (parse_value(int, 16), parse_value(f, 16)) {
                        (Some(i), Some(f)) => Some(shifted_cost(i, 0) + shifted_cost(f, e)),
                        _ => None,
                    },
                    None => parse_value(int, 16).map(|i| shifted_cost(i, e)),
                }
            }
            TokenKind::Number { is_hex: false, int, frac, exp } => {
                let int_cost = digits_cost(int, 10).unwrap_or(0);
                let frac_cost = match frac {
                    Some(f) => digits_cost(f, 10).unwrap_or(1),
                    None => 0,
                };
                let exp_cost = match exp {
                    Some(s) => digits_cost(s, 10).unwrap_or(0),
                    None => 0,
                };
                Some(int_cost + frac_cost + exp_cost)
            }
            TokenKind::Ident => {
                Some(if self.is_after_assignment_lhs_comma { 1usize << (self.text.len() / 2) } else { 1 })
            }
            TokenKind::ClosingBracket => Some(0),
            // An open brace (table constructor) costs the same as other opening punctuation.
            TokenKind::OpenBrace | TokenKind::Punct => Some(1),
        }
    }
}

fn digits_cost(digits: &str, radix: u32) -> Option<usize> {
    Some(shifted_cost(u64::from_str_radix(digits, radix).ok()?, 0))
}

fn parse_value(digits: &str, radix: u32) -> Option<u64> {
    if digits.is_empty() { Some(0) } else { u64::from_str_radix(digits, radix).ok() }
}

// Cost of the value `m * 2^e`.
fn shifted_cost(m: u64, e: u64) -> usize {
    if m == 0 || (m == 1 && e == 0) {
        return 1;
    }
    let log2 = (m.ilog2() as u64).saturating_add(e);
    let top_bit = log2 - u64::from(m.is_power_of_two());
    1usize << (top_bit / 4).min(15)
}
