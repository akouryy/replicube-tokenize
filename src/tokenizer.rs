use crate::token::{Token, TokenKind};

const MULTI_CHAR_PUNCT_2: &[&[u8]] = &[b"==", b"~=", b"<=", b">=", b"..", b"::", b"<<", b">>", b"//"];

pub fn tokenize(src: &str) -> Vec<Token<'_>> {
    Lexer::new(src).collect()
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    // Whether the previous token ends a value, which decides whether a
    // following `-` is binary subtraction (true) or unary negation (false).
    does_prev_end_value: bool,
    // Whether the previous token is a comma that separates assignment targets,
    // which makes the next variable charged by its name length.
    is_after_lhs_comma: bool,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, bytes: src.as_bytes(), pos: 0, does_prev_end_value: false, is_after_lhs_comma: false }
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

    // Long brackets (`--[[ ]]`) are unsupported, so a comment always runs to end of line.
    fn skip_comment(&mut self) {
        self.pos += 2;
        while let Some(b) = self.peek() {
            self.pos += 1;
            if b == b'\n' {
                return;
            }
        }
    }

    fn read_string(&mut self, quote: u8) -> TokenKind<'a> {
        let content_start = self.pos + 1;
        self.pos += 1;
        while let Some(b) = self.peek() {
            if b == quote {
                let content = &self.src[content_start..self.pos];
                self.pos += 1;
                return TokenKind::Str(content);
            }
            if b == b'\\' && self.peek_at(1).is_some() {
                self.pos += 2;
            } else {
                self.pos += 1;
            }
        }
        // Unterminated: the content runs to end of input.
        TokenKind::Str(&self.src[content_start..self.pos])
    }

    fn read_number(&mut self) -> TokenKind<'a> {
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
        let int = self.scan_slice(digit_marker);
        let frac = (self.peek() == Some(b'.')).then(|| {
            self.pos += 1;
            self.scan_slice(digit_marker)
        });
        let exp = self.read_exponent(is_hex, exponent_marker);
        TokenKind::Number { is_hex, int, frac, exp }
    }

    // Read an exponent suffix, returning its digit run. An exponent must begin with a digit; a
    // sign is handled differently per base, and an unhandled sign splits the literal so the
    // leftover `sign digits` lex as separate tokens. Hex `p` accepts no sign (but the marker is
    // still consumed); decimal `e` accepts a leading `-` but not `+`.
    fn read_exponent(&mut self, is_hex: bool, marker: [u8; 2]) -> Option<&'a str> {
        if !self.peek().is_some_and(|b| marker.contains(&b)) {
            return None;
        }
        if is_hex {
            self.pos += 1;
            self.peek().is_some_and(|b| b.is_ascii_digit()).then(|| self.scan_slice(|b| b.is_ascii_digit()))
        } else {
            let is_signed = self.peek_at(1) == Some(b'-') && self.peek_at(2).is_some_and(|b| b.is_ascii_digit());
            (is_signed || self.peek_at(1).is_some_and(|b| b.is_ascii_digit())).then(|| {
                self.pos += 1;
                if self.peek() == Some(b'-') {
                    self.pos += 1;
                }
                self.scan_slice(|b| b.is_ascii_digit())
            })
        }
    }

    fn scan_while<F: Fn(u8) -> bool>(&mut self, pred: F) {
        while let Some(b) = self.peek() {
            if !pred(b) {
                return;
            }
            self.pos += 1;
        }
    }

    // Advance over bytes matching `pred` and return the slice consumed.
    fn scan_slice<F: Fn(u8) -> bool>(&mut self, pred: F) -> &'a str {
        let start = self.pos;
        self.scan_while(pred);
        &self.src[start..self.pos]
    }

    fn read_word(&mut self) -> TokenKind<'a> {
        self.scan_while(|b| b.is_ascii_alphanumeric() || b == b'_');
        TokenKind::Ident
    }

    fn read_punct(&mut self) -> TokenKind<'a> {
        let start = self.pos;
        self.pos += self.punct_width();
        match &self.src[start..self.pos] {
            ")" | "]" | "}" => TokenKind::ClosingBracket,
            "{" => TokenKind::OpenBrace,
            "(" | "[" | "+" | "-" | "*" | "/" | "%" | "^" | "#" | "&" | "~" | "|" | "<" | ">"
            | "=" | ";" | ":" | "," | "." | "==" | "~=" | "<=" | ">=" | ".." | "::" | "<<"
            | ">>" | "//" | "..." => TokenKind::Punct,
            _ => TokenKind::Unknown,
        }
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

    // Whether a `-` directly following the cursor begins a number literal (`-2`, `-.5`).
    fn does_minus_start_number(&self) -> bool {
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

impl<'a> Iterator for Lexer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        self.skip_trivia();
        let b = self.peek()?;
        let is_after_lhs_comma = self.is_after_lhs_comma;
        let start = self.pos;
        let kind = match b {
            b'"' | b'\'' => self.read_string(b),
            b if b.is_ascii_digit() => self.read_number(),
            b'.' if self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) => self.read_number(),
            b'-' if !self.does_prev_end_value && self.does_minus_start_number() => self.read_number(),
            b if b.is_ascii_alphabetic() || b == b'_' => self.read_word(),
            _ => self.read_punct(),
        };
        let token = Token { text: &self.src[start..self.pos], is_after_lhs_comma, kind };
        self.does_prev_end_value = does_token_end_value(&token);
        // A comma separates assignment targets when the rest forms `Name (, Name)*`
        // ending with a single `=`; the next variable is then charged by length.
        self.is_after_lhs_comma = token.text == "," && self.is_lhs_comma(self.pos);
        Some(token)
    }
}

// Whether a token ends a value, so a following `-` is subtraction rather than a unary sign.
fn does_token_end_value(token: &Token) -> bool {
    match token.kind {
        TokenKind::Str(_) | TokenKind::Number { .. } | TokenKind::ClosingBracket => true,
        TokenKind::Ident => does_word_end_value(token.text),
        TokenKind::Punct | TokenKind::OpenBrace | TokenKind::Unknown => false,
    }
}

// Whether a word token (identifier or keyword) ends a value. Keywords that expect an expression
// after them keep `-` unary; all other words (identifiers and value keywords like `true`/`nil`)
// end a value.
fn does_word_end_value(text: &str) -> bool {
    !matches!(
        text,
        "and" | "or" | "not" | "if" | "elseif" | "else" | "then" | "do" | "while" | "repeat"
            | "until" | "for" | "in" | "return" | "function" | "local" | "goto" | "break"
    )
}
