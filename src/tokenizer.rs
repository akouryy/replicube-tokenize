#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    /// Token cost, or `None` when the cost is not yet determined
    /// (string literals, table constructors, non-integer numbers).
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
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, bytes: src.as_bytes(), pos: 0, prev_ends_value: false }
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
        let mut has_exponent = false;
        if matches!(self.peek(), Some(b) if b == exponent_marker[0] || b == exponent_marker[1]) {
            has_exponent = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            self.scan_while(|b| b.is_ascii_digit());
        }
        let radix = if is_hex { 16 } else { 10 };
        let cost = if has_exponent {
            // Scientific notation cost is undetermined.
            None
        } else if let Some((frac_start, frac_end)) = frac {
            // Decimal `a.b` costs cost(a) + cost(b); a missing `a` or `b` contributes nothing.
            let int_cost = part_cost(&self.src[int_start..int_end], radix);
            let frac_cost = part_cost(&self.src[frac_start..frac_end], radix);
            match (int_cost, frac_cost) {
                (Some(a), Some(b)) => Some(a + b),
                _ => None,
            }
        } else {
            digits_cost(&self.src[int_start..int_end], radix)
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
        let token = match b {
            b'"' | b'\'' => self.read_short_string(b),
            b'[' if self.peek_long_bracket_open().is_some() => self.read_long_string(),
            b if b.is_ascii_digit() => self.read_number(),
            b'.' if self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) => self.read_number(),
            b'-' if !self.prev_ends_value && self.minus_starts_number() => self.read_number(),
            b if b.is_ascii_alphabetic() || b == b'_' => self.read_word(),
            _ => self.read_punct(),
        };
        self.prev_ends_value = token_ends_value(&token.text);
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
}

// Whether a token can end a value expression, so a following `-` is subtraction.
fn token_ends_value(text: &str) -> bool {
    match text {
        ")" | "]" | "}" => true,
        // Keywords that expect an expression after them keep `-` unary.
        "and" | "or" | "not" | "if" | "elseif" | "else" | "then" | "do" | "while" | "repeat"
        | "until" | "for" | "in" | "return" | "function" | "local" | "goto" | "break" => false,
        _ => {
            let first = text.as_bytes()[0];
            // Identifiers, value keywords, numbers, and strings end a value;
            // other punctuation (operators, `(`, `[`, `{`, `,`, `=`, ...) does not.
            first.is_ascii_alphanumeric() || first == b'_' || first == b'"' || first == b'\'' || first == b'['
        }
    }
}

// Cost of one part of a decimal `a.b`; an empty part contributes nothing.
fn part_cost(digits: &str, radix: u32) -> Option<usize> {
    if digits.is_empty() { Some(0) } else { digits_cost(digits, radix) }
}

fn digits_cost(digits: &str, radix: u32) -> Option<usize> {
    match u64::from_str_radix(digits, radix).ok() {
        // Cost doubles past each power of 16: 1..=16 -> 1, 17..=256 -> 2, 257..=4096 -> 4, ...
        Some(0 | 1) => Some(1),
        Some(v) => Some(1usize << ((v - 1).ilog2() / 4)),
        None => None,
    }
}
