#[derive(Debug, Clone)]
pub struct Token<'a> {
    /// The token's source text.
    pub text: &'a str,
    /// Whether this token follows a comma that separates assignment targets,
    /// which charges an identifier by its name length.
    pub(crate) is_after_lhs_comma: bool,
    pub kind: TokenKind<'a>,
}

#[derive(Debug, Clone)]
pub enum TokenKind<'a> {
    /// String literal; carries the content between the quotes.
    Str(&'a str),
    /// Numeric literal, split into its digit runs.
    Number {
        is_hex: bool,
        int: &'a str,
        frac: Option<&'a str>,
        exp: Option<&'a str>,
    },
    /// Identifier or keyword.
    Ident,
    /// `)`, `]`, or `}`.
    ClosingBracket,
    /// Any other operator or separator.
    Punct,
    /// `{` (table constructor).
    OpenBrace,
    /// An unrecognized byte.
    Unknown,
}

impl Token<'_> {
    /// Token cost, or `None` when the cost is undetermined (string literals,
    /// table constructors, unrecognized input).
    pub fn cost(&self) -> Option<usize> {
        match &self.kind {
            TokenKind::Str(_) | TokenKind::OpenBrace | TokenKind::Unknown => None,
            TokenKind::Number { is_hex: true, int, frac, exp } => {
                // Hex exponents scale the value by `2^E` instead of adding a digit cost. A bare
                // integer costs `dc(I * 2^E)`; with a fractional part the integer part is frozen
                // and the exponent applies to the fraction: `dc(I) + dc(F * 2^E)`.
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
                // A decimal number costs the sum of its parts: integer + fractional + exponent. An
                // empty integer/exponent part contributes 0; an empty fractional part still costs 1
                // when a decimal point is present.
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
            // A variable after a comma in an assignment target list is charged by its name
            // length: 1 char -> 1, 2-3 -> 2, 4-5 -> 4, ... (2^(len/2)); otherwise a flat 1.
            TokenKind::Ident => {
                Some(if self.is_after_lhs_comma { 1usize << (self.text.len() / 2) } else { 1 })
            }
            // Closing brackets are free.
            TokenKind::ClosingBracket => Some(0),
            TokenKind::Punct => Some(1),
        }
    }
}

fn digits_cost(digits: &str, radix: u32) -> Option<usize> {
    Some(shifted_cost(u64::from_str_radix(digits, radix).ok()?, 0))
}

// Parse a digit substring to its value; an empty string is 0 and an overflow is None.
fn parse_value(digits: &str, radix: u32) -> Option<u64> {
    if digits.is_empty() { Some(0) } else { u64::from_str_radix(digits, radix).ok() }
}

// Cost of the value `m * 2^e` (m may be 0), saturating at 1 << 15 the way a u64 value would.
// Cost doubles past each power of 16: 0..=16 -> 1, 17..=256 -> 2, 257..=4096 -> 4, ...
fn shifted_cost(m: u64, e: u64) -> usize {
    // 0 and 1 cost 1; `0 * 2^e` is 0, which also costs 1.
    if m == 0 || (m == 1 && e == 0) {
        return 1;
    }
    // v = m << e >= 2; (v - 1).ilog2() is ilog2(m) + e, less 1 when m is a power of two.
    let log2 = (m.ilog2() as u64).saturating_add(e);
    let top_bit = log2 - u64::from(m.is_power_of_two());
    1usize << (top_bit / 4).min(15)
}
