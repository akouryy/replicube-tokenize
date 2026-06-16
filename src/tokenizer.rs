#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    /// Token cost, or `None` when the cost is not yet determined
    /// (string literals, table constructors).
    pub cost: Option<usize>,
}

const MULTI_CHAR_PUNCT_2: &[&[u8]] = &[b"==", b"~=", b"<=", b">=", b"..", b"::", b"<<", b">>", b"//"];

pub fn tokenize(src: &str) -> Vec<Token> {
    Lexer::new(src).collect()
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    // Whether the previous token ends a value, which decides whether a
    // following `-` is binary subtraction (true) or unary negation (false).
    prev_ends_value: bool,
    // Whether the previous token is a comma that separates assignment targets,
    // which makes the next variable charged by its name length.
    after_lhs_comma: bool,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, bytes: src.as_bytes(), pos: 0, prev_ends_value: false, after_lhs_comma: false }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(b) if b.is_ascii_whitespace() => self.pos += 1,
                Some(b'-') if self.peek_at(1) == Some(b'-') => self.skip_comment(),
                _ => return,
            }
        }
    }

    fn skip_comment(&mut self) {
        self.pos += 2;
        if let Some(level) = self.consume_long_bracket_open() {
            self.skip_to_long_bracket_close(level);
        } else {
            while let Some(b) = self.peek() {
                self.pos += 1;
                if b == b'\n' {
                    return;
                }
            }
        }
    }

    // If the cursor is at a long-bracket opener `[={level}[`, advance past it and return the level.
    fn consume_long_bracket_open(&mut self) -> Option<usize> {
        let level = self.peek_long_bracket_open()?;
        self.pos += level + 2;
        Some(level)
    }

    fn peek_long_bracket_open(&self) -> Option<usize> {
        if self.peek() != Some(b'[') {
            return None;
        }
        let mut j = self.pos + 1;
        while self.bytes.get(j) == Some(&b'=') {
            j += 1;
        }
        if self.bytes.get(j) == Some(&b'[') { Some(j - self.pos - 1) } else { None }
    }

    fn skip_to_long_bracket_close(&mut self, level: usize) {
        while self.pos < self.bytes.len() {
            if self.bytes[self.pos] == b']' {
                let mut j = self.pos + 1;
                let mut count = 0;
                while self.bytes.get(j) == Some(&b'=') {
                    j += 1;
                    count += 1;
                }
                if count == level && self.bytes.get(j) == Some(&b']') {
                    self.pos = j + 1;
                    return;
                }
            }
            self.pos += 1;
        }
    }

    fn read_short_string(&mut self, quote: u8) -> Token {
        let start = self.pos;
        self.pos += 1;
        while let Some(b) = self.peek() {
            if b == quote {
                self.pos += 1;
                break;
            }
            if b == b'\\' && self.peek_at(1).is_some() {
                self.pos += 2;
            } else {
                self.pos += 1;
            }
        }
        self.string_token(start)
    }

    fn read_long_string(&mut self) -> Token {
        let start = self.pos;
        let level = self.consume_long_bracket_open().expect("dispatcher already verified a long-bracket opener");
        self.skip_to_long_bracket_close(level);
        self.string_token(start)
    }

    fn string_token(&self, start: usize) -> Token {
        Token {
            text: self.src[start..self.pos].to_string(),
            // String literal cost is undetermined.
            cost: None,
        }
    }

    fn read_number(&mut self) -> Token {
        let start = self.pos;
        // A unary minus is absorbed into the literal; its sign does not affect cost.
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let is_hex = self.peek() == Some(b'0') && matches!(self.peek_at(1), Some(b'x' | b'X'));
        let (digit_marker, exponent_marker): (fn(u8) -> bool, [u8; 2]) =
            if is_hex { (|b| b.is_ascii_hexdigit(), [b'p', b'P']) } else { (|b| b.is_ascii_digit(), [b'e', b'E']) };
        if is_hex {
            self.pos += 2;
        }
        let int_start = self.pos;
        self.scan_while(digit_marker);
        let int_end = self.pos;
        let mut frac: Option<(usize, usize)> = None;
        if self.peek() == Some(b'.') {
            self.pos += 1;
            let frac_start = self.pos;
            self.scan_while(digit_marker);
            frac = Some((frac_start, self.pos));
        }
        // An exponent must begin with a digit; a sign is handled differently per base,
        // and an unhandled sign splits the literal so the leftover `sign digits` lex as
        // separate tokens. Hex `p` accepts no sign (but the marker is still consumed);
        // decimal `e` accepts a leading `-` but not `+`.
        let mut exponent: Option<(usize, usize)> = None;
        if matches!(self.peek(), Some(b) if b == exponent_marker[0] || b == exponent_marker[1]) {
            if is_hex {
                self.pos += 1;
                if self.peek().is_some_and(|b| b.is_ascii_digit()) {
                    let exp_start = self.pos;
                    self.scan_while(|b| b.is_ascii_digit());
                    exponent = Some((exp_start, self.pos));
                }
            } else {
                let signed = self.peek_at(1) == Some(b'-') && self.peek_at(2).is_some_and(|b| b.is_ascii_digit());
                if signed || self.peek_at(1).is_some_and(|b| b.is_ascii_digit()) {
                    self.pos += 1;
                    if self.peek() == Some(b'-') {
                        self.pos += 1;
                    }
                    let exp_start = self.pos;
                    self.scan_while(|b| b.is_ascii_digit());
                    exponent = Some((exp_start, self.pos));
                }
            }
        }
        let cost = if is_hex {
            // Hex exponents scale the value by `2^E` instead of adding a digit cost.
            // A bare integer costs `dc(I * 2^E)`; a literal with a fractional part freezes
            // the integer part and applies the exponent to the fraction: `dc(I) + dc(F * 2^E)`.
            let exp = match exponent {
                Some((exp_start, exp_end)) => parse_value(&self.src[exp_start..exp_end], 10).unwrap_or(u64::MAX),
                None => 0,
            };
            match frac {
                Some((frac_start, frac_end)) => {
                    let int_val = parse_value(&self.src[int_start..int_end], 16);
                    let frac_val = parse_value(&self.src[frac_start..frac_end], 16);
                    match (int_val, frac_val) {
                        (Some(i), Some(f)) => Some(shifted_cost(i, 0) + shifted_cost(f, exp)),
                        _ => None,
                    }
                }
                None => parse_value(&self.src[int_start..int_end], 16).map(|i| shifted_cost(i, exp)),
            }
        } else {
            // A decimal number costs the sum of its parts: integer + fractional + exponent.
            // An empty integer/exponent part contributes 0; an empty fractional part
            // still costs 1 when a decimal point is present.
            let int_cost = digits_cost(&self.src[int_start..int_end], 10).unwrap_or(0);
            let frac_cost = match frac {
                Some((frac_start, frac_end)) => digits_cost(&self.src[frac_start..frac_end], 10).unwrap_or(1),
                None => 0,
            };
            let exp_cost = match exponent {
                Some((exp_start, exp_end)) => digits_cost(&self.src[exp_start..exp_end], 10).unwrap_or(0),
                None => 0,
            };
            Some(int_cost + frac_cost + exp_cost)
        };
        Token { text: self.src[start..self.pos].to_string(), cost }
    }

    fn scan_while<F: Fn(u8) -> bool>(&mut self, pred: F) {
        while let Some(b) = self.peek() {
            if !pred(b) {
                return;
            }
            self.pos += 1;
        }
    }

    fn read_word(&mut self) -> Token {
        let start = self.pos;
        self.scan_while(|b| b.is_ascii_alphanumeric() || b == b'_');
        Token { text: self.src[start..self.pos].to_string(), cost: Some(1) }
    }

    fn read_punct(&mut self) -> Token {
        let start = self.pos;
        let width = self.punct_width();
        self.pos += width;
        let text = self.src[start..self.pos].to_string();
        let cost = match text.as_str() {
            // Closing brackets are free.
            ")" | "]" | "}" => Some(0),
            // Table constructor cost is undetermined.
            "{" => None,
            "(" | "[" | "+" | "-" | "*" | "/" | "%" | "^" | "#" | "&" | "~" | "|" | "<" | ">"
            | "=" | ";" | ":" | "," | "." | "==" | "~=" | "<=" | ">=" | ".." | "::" | "<<"
            | ">>" | "//" | "..." => Some(1),
            // Unrecognized character: cost is undetermined.
            _ => None,
        };
        Token { text, cost }
    }

    fn punct_width(&self) -> usize {
        if self.bytes.get(self.pos..self.pos + 3) == Some(b"..." as &[u8]) {
            return 3;
        }
        if let Some(two) = self.bytes.get(self.pos..self.pos + 2)
            && MULTI_CHAR_PUNCT_2.contains(&two)
        {
            return 2;
        }
        1
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        self.skip_trivia();
        let b = self.peek()?;
        let after_lhs_comma = self.after_lhs_comma;
        // The match arm already determines the token kind, so we record whether it
        // ends a value directly instead of re-deriving the kind from the token text.
        // Numbers and strings always end a value (the leading `-`/`.` a number may
        // carry would otherwise be misread as "not a value").
        let (ends_value, token) = match b {
            b'"' | b'\'' => (true, self.read_short_string(b)),
            b'[' if self.peek_long_bracket_open().is_some() => (true, self.read_long_string()),
            b if b.is_ascii_digit() => (true, self.read_number()),
            b'.' if self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) => (true, self.read_number()),
            b'-' if !self.prev_ends_value && self.minus_starts_number() => (true, self.read_number()),
            b if b.is_ascii_alphabetic() || b == b'_' => {
                let mut token = self.read_word();
                // A variable after a comma in an assignment target list is charged by
                // its name length: 1 char -> 1, 2-3 -> 2, 4-5 -> 4, ... (2^(len/2)).
                if after_lhs_comma {
                    token.cost = Some(1usize << (token.text.len() / 2));
                }
                (word_ends_value(&token.text), token)
            }
            // Only closing brackets end a value; other punctuation does not.
            _ => {
                let token = self.read_punct();
                (matches!(token.text.as_str(), ")" | "]" | "}"), token)
            }
        };
        self.prev_ends_value = ends_value;
        // A comma separates assignment targets when the rest forms `Name (, Name)*`
        // ending with a single `=`; the next variable is then charged by length.
        self.after_lhs_comma = token.text == "," && self.is_lhs_comma(self.pos);
        Some(token)
    }
}

// Whether a `-` directly following the cursor begins a number literal (`-2`, `-.5`).
impl Lexer<'_> {
    fn minus_starts_number(&self) -> bool {
        match self.peek_at(1) {
            Some(c) if c.is_ascii_digit() => true,
            Some(b'.') => self.peek_at(2).is_some_and(|c| c.is_ascii_digit()),
            _ => false,
        }
    }

    fn skip_ws_at(&self, mut pos: usize) -> usize {
        while matches!(self.bytes.get(pos), Some(b) if b.is_ascii_whitespace()) {
            pos += 1;
        }
        pos
    }

    // Whether the comma just consumed (cursor now at `pos`) separates assignment
    // targets, i.e. the rest forms `Name (, Name)*` terminated by a single `=`.
    fn is_lhs_comma(&self, mut pos: usize) -> bool {
        loop {
            pos = self.skip_ws_at(pos);
            let start = pos;
            while matches!(self.bytes.get(pos), Some(b) if b.is_ascii_alphanumeric() || *b == b'_') {
                pos += 1;
            }
            // Must be a Name: non-empty and not starting with a digit.
            if pos == start || self.bytes[start].is_ascii_digit() {
                return false;
            }
            pos = self.skip_ws_at(pos);
            match self.bytes.get(pos) {
                Some(b'=') if self.bytes.get(pos + 1) != Some(&b'=') => return true,
                Some(b',') => pos += 1,
                _ => return false,
            }
        }
    }
}

// Whether a word token (identifier or keyword) ends a value, so a following `-` is
// subtraction. Keywords that expect an expression after them keep `-` unary; all other
// words (identifiers and value keywords like `true`/`nil`) end a value.
fn word_ends_value(text: &str) -> bool {
    !matches!(
        text,
        "and" | "or" | "not" | "if" | "elseif" | "else" | "then" | "do" | "while" | "repeat"
            | "until" | "for" | "in" | "return" | "function" | "local" | "goto" | "break"
    )
}

fn digits_cost(digits: &str, radix: u32) -> Option<usize> {
    match u64::from_str_radix(digits, radix).ok() {
        // Cost doubles past each power of 16: 1..=16 -> 1, 17..=256 -> 2, 257..=4096 -> 4, ...
        Some(0 | 1) => Some(1),
        Some(v) => Some(1usize << ((v - 1).ilog2() / 4)),
        None => None,
    }
}

// Parse a digit substring to its value; an empty string is 0 and an overflow is None.
fn parse_value(digits: &str, radix: u32) -> Option<u64> {
    if digits.is_empty() { Some(0) } else { u64::from_str_radix(digits, radix).ok() }
}

// Cost of the value `m * 2^e` (m may be 0), saturating at 1 << 15 the way a u64 value would.
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
