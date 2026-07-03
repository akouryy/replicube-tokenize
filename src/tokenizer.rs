use crate::token::{Token, TokenKind};
use crate::warning::Warning;

const MULTI_CHAR_PUNCT_2: &[&[u8]] = &[b"==", b"~=", b"<=", b">=", b"..", b"::", b"<<", b">>", b"//", b",["];

pub fn tokenize(src: &str) -> (Vec<Token<'_>>, Vec<Warning>) {
    let tokens: Vec<_> = Lexer::new(src).collect();
    let warnings = collect_warnings(src, &tokens);
    (tokens, warnings)
}

fn collect_warnings(src: &str, tokens: &[Token<'_>]) -> Vec<Warning> {
    let mut warnings = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if matches!(token.kind, TokenKind::Semicolon) {
            warnings.push(Warning::Semicolon { pos: byte_offset(src, token) });
        }
        if matches!(token.kind, TokenKind::Comma) && tokens.get(i + 1).is_some_and(|t| matches!(t.kind, TokenKind::OpenBracket))
        {
            warnings.push(Warning::WhitespaceBetweenCommaAndBracket { pos: byte_offset(src, token) });
        }
    }
    warnings
}

fn byte_offset(src: &str, token: &Token) -> usize { token.text.as_ptr() as usize - src.as_ptr() as usize }

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    // When set, a following `-` is subtraction, not negation.
    is_after_expr_node: bool,
    is_after_assignment_lhs_comma: bool,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, bytes: src.as_bytes(), pos: 0, is_after_expr_node: false, is_after_assignment_lhs_comma: false }
    }

    fn peek(&self) -> Option<u8> { self.bytes.get(self.pos).copied() }

    fn peek_at(&self, offset: usize) -> Option<u8> { self.bytes.get(self.pos + offset).copied() }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(b) if b.is_ascii_whitespace() => self.pos += 1,
                Some(b'-') if self.peek_at(1) == Some(b'-') => self.skip_comment(),
                _ => return,
            }
        }
    }

    // Long brackets (`--[[ ]]`) are unsupported, so a comment runs to end of line.
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
        TokenKind::Str(&self.src[content_start..self.pos])
    }

    fn read_number(&mut self) -> TokenKind<'a> {
        // A unary minus (but not plus) is absorbed into the literal.
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

    // Read an exponent suffix, returning its digit run. It must begin with a digit; an unhandled
    // sign splits the literal so the leftover `sign digits` lex separately. Hex `p` accepts no
    // sign (the marker is still consumed); decimal `e` accepts a leading `-` but not `+`.
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
            "[" => TokenKind::OpenBracket,
            ",[" => TokenKind::CommaOpenBracket,
            "," => TokenKind::Comma,
            ";" => TokenKind::Semicolon,
            "(" | "+" | "-" | "*" | "/" | "%" | "^" | "#" | "&" | "~" | "|" | "<" | ">" | "=" | ":" | "." | "==" | "~="
            | "<=" | ">=" | ".." | "::" | "<<" | ">>" | "//" | "..." => TokenKind::Punct,
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

    fn is_assignment_lhs_comma(&self, mut pos: usize) -> bool {
        loop {
            pos = self.skip_ws_at(pos);
            let start = pos;
            while matches!(self.bytes.get(pos), Some(b) if b.is_ascii_alphanumeric() || *b == b'_') {
                pos += 1;
            }
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
        let is_after_assignment_lhs_comma = self.is_after_assignment_lhs_comma;
        let start = self.pos;
        let kind = match b {
            b'"' | b'\'' => self.read_string(b),
            b if b.is_ascii_digit() => self.read_number(),
            b'.' if self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) => self.read_number(),
            b'-' if !self.is_after_expr_node && self.does_minus_start_number() => self.read_number(),
            b if b.is_ascii_alphabetic() || b == b'_' => self.read_word(),
            _ => self.read_punct(),
        };
        let token = Token { text: &self.src[start..self.pos], is_after_assignment_lhs_comma, kind };
        self.is_after_expr_node = does_token_end_value(&token);
        self.is_after_assignment_lhs_comma = matches!(token.kind, TokenKind::Comma) && self.is_assignment_lhs_comma(self.pos);
        Some(token)
    }
}

fn does_token_end_value(token: &Token) -> bool {
    match token.kind {
        TokenKind::Str(_) | TokenKind::Number { .. } | TokenKind::ClosingBracket => true,
        TokenKind::Ident => does_word_end_value(token.text),
        // Replicube quirk: `[` and `,[` are (buggily) treated as ending a value, so a following `-` lexes as subtraction rather than a sign; e.g. `[-5]` costs `[`, `-`, `5` separately.
        TokenKind::OpenBracket | TokenKind::CommaOpenBracket => true,
        TokenKind::Punct | TokenKind::OpenBrace | TokenKind::Comma | TokenKind::Semicolon | TokenKind::Unknown => false,
    }
}

fn does_word_end_value(text: &str) -> bool {
    !matches!(
        text,
        "and"
            | "or"
            | "not"
            | "if"
            | "elseif"
            | "else"
            | "then"
            | "do"
            | "while"
            | "repeat"
            | "until"
            | "for"
            | "in"
            | "return"
            | "function"
            | "local"
            | "goto"
            | "break"
    )
}
