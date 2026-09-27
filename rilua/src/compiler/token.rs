//! Token types produced by the lexer.
//!
//! Token variants follow PUC-Rio's ordering: 21 reserved words (alphabetical),
//! 6 multi-character operators, 3 literal types, single-character tokens via
//! `Char(u8)`, and `Eos` for end-of-stream.

use std::fmt;

/// Source position for error reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Line number (1-based).
    pub line: u32,
    /// Column number (1-based).
    pub column: u32,
}

impl Span {
    /// Creates a new span at the given line and column.
    #[must_use]
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

/// A token produced by the lexer.
///
/// Multi-token variants carry their own data (numeric value, string content,
/// identifier name). Single-character tokens (like `+`, `-`, `(`) use the
/// `Char(u8)` variant with the raw ASCII byte value, matching PUC-Rio's
/// approach where single-char tokens are returned as their integer value.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // -- Reserved words (21, alphabetical order matching PUC-Rio) --
    /// `and`
    And,
    /// `break`
    Break,
    /// `do`
    Do,
    /// `else`
    Else,
    /// `elseif`
    ElseIf,
    /// `end`
    End,
    /// `false`
    False,
    /// `for`
    For,
    /// `function`
    Function,
    /// `if`
    If,
    /// `in`
    In,
    /// `local`
    Local,
    /// `nil`
    Nil,
    /// `not`
    Not,
    /// `or`
    Or,
    /// `repeat`
    Repeat,
    /// `return`
    Return,
    /// `then`
    Then,
    /// `true`
    True,
    /// `until`
    Until,
    /// `while`
    While,

    // -- Multi-character operators --
    /// `..` (string concatenation)
    Concat,
    /// `...` (varargs)
    Dots,
    /// `==`
    Eq,
    /// `>=`
    Ge,
    /// `<=`
    Le,
    /// `~=`
    Ne,

    // -- Literals --
    /// Numeric literal (always f64 in Lua 5.1).
    Number(f64),
    /// Identifier.
    Name(String),
    /// String literal (after escape processing). Stored as raw bytes
    /// because Lua strings can contain arbitrary byte values including `\0`.
    Str(Vec<u8>),

    // -- Single-character tokens --
    /// Any single-character token (e.g., `+`, `-`, `(`, `)`, `=`, etc.).
    /// Stored as the raw ASCII byte value.
    Char(u8),

    // -- End of stream --
    /// End of input.
    Eos,
}

/// All 21 reserved words in PUC-Rio order (alphabetical).
/// Used by tests to validate `lookup_keyword` correctness.
#[cfg(test)]
const RESERVED_WORDS: &[(&str, Token)] = &[
    ("and", Token::And),
    ("break", Token::Break),
    ("do", Token::Do),
    ("else", Token::Else),
    ("elseif", Token::ElseIf),
    ("end", Token::End),
    ("false", Token::False),
    ("for", Token::For),
    ("function", Token::Function),
    ("if", Token::If),
    ("in", Token::In),
    ("local", Token::Local),
    ("nil", Token::Nil),
    ("not", Token::Not),
    ("or", Token::Or),
    ("repeat", Token::Repeat),
    ("return", Token::Return),
    ("then", Token::Then),
    ("true", Token::True),
    ("until", Token::Until),
    ("while", Token::While),
];

/// O(1) keyword lookup dispatching on (length, first byte).
///
/// Compiles to a jump table. At most one `memcmp` per call.
/// Returns `None` for non-keyword identifiers.
#[inline]
pub(crate) fn lookup_keyword(bytes: &[u8]) -> Option<Token> {
    let &first = bytes.first()?;
    match (bytes.len(), first) {
        (2, b'd') if bytes == b"do" => Some(Token::Do),
        (2, b'i') => {
            if bytes[1] == b'f' {
                Some(Token::If)
            } else if bytes[1] == b'n' {
                Some(Token::In)
            } else {
                None
            }
        }
        (2, b'o') if bytes[1] == b'r' => Some(Token::Or),
        (3, b'a') if bytes == b"and" => Some(Token::And),
        (3, b'e') if bytes == b"end" => Some(Token::End),
        (3, b'f') if bytes == b"for" => Some(Token::For),
        (3, b'n') => {
            if bytes == b"nil" {
                Some(Token::Nil)
            } else if bytes == b"not" {
                Some(Token::Not)
            } else {
                None
            }
        }
        (4, b'e') if bytes == b"else" => Some(Token::Else),
        (4, b't') => {
            if bytes == b"then" {
                Some(Token::Then)
            } else if bytes == b"true" {
                Some(Token::True)
            } else {
                None
            }
        }
        (5, b'b') if bytes == b"break" => Some(Token::Break),
        (5, b'f') if bytes == b"false" => Some(Token::False),
        (5, b'l') if bytes == b"local" => Some(Token::Local),
        (5, b'u') if bytes == b"until" => Some(Token::Until),
        (5, b'w') if bytes == b"while" => Some(Token::While),
        (6, b'e') if bytes == b"elseif" => Some(Token::ElseIf),
        (6, b'r') => {
            if bytes == b"repeat" {
                Some(Token::Repeat)
            } else if bytes == b"return" {
                Some(Token::Return)
            } else {
                None
            }
        }
        (8, b'f') if bytes == b"function" => Some(Token::Function),
        _ => None,
    }
}

impl Token {
    /// Returns the unquoted token name for error messages.
    ///
    /// Matches PUC-Rio's `luaX_token2str`: returns the raw string without
    /// surrounding quotes.
    /// - Reserved words: `and`, `break`, `do`, ...
    /// - Multi-char operators: `..`, `...`, `==`, ...
    /// - Literals: `<name>`, `<number>`, `<string>`
    /// - Single chars: the character itself (`;`, `+`, `-`, ...)
    /// - End of stream: `<eof>`
    #[must_use]
    pub fn token2str(&self) -> String {
        match self {
            // Reserved words
            Self::And => "and".into(),
            Self::Break => "break".into(),
            Self::Do => "do".into(),
            Self::Else => "else".into(),
            Self::ElseIf => "elseif".into(),
            Self::End => "end".into(),
            Self::False => "false".into(),
            Self::For => "for".into(),
            Self::Function => "function".into(),
            Self::If => "if".into(),
            Self::In => "in".into(),
            Self::Local => "local".into(),
            Self::Nil => "nil".into(),
            Self::Not => "not".into(),
            Self::Or => "or".into(),
            Self::Repeat => "repeat".into(),
            Self::Return => "return".into(),
            Self::Then => "then".into(),
            Self::True => "true".into(),
            Self::Until => "until".into(),
            Self::While => "while".into(),
            // Multi-char operators
            Self::Concat => "..".into(),
            Self::Dots => "...".into(),
            Self::Eq => "==".into(),
            Self::Ge => ">=".into(),
            Self::Le => "<=".into(),
            Self::Ne => "~=".into(),
            // Literals
            Self::Number(_) => "<number>".into(),
            Self::Name(_) => "<name>".into(),
            Self::Str(_) => "<string>".into(),
            // Single chars (iscntrl check matches PUC-Rio)
            Self::Char(c) if c.is_ascii_control() => format!("char({c})"),
            Self::Char(c) => format!("{}", char::from(*c)),
            // End of stream
            Self::Eos => "<eof>".into(),
        }
    }

    /// Returns the token name quoted with single quotes for error messages.
    ///
    /// Wraps `token2str()` result with `'...'` (PUC-Rio's `LUA_QS` pattern).
    /// Used in `error_expected` and similar contexts.
    #[must_use]
    pub fn display_name(&self) -> String {
        format!("'{}'", self.token2str())
    }

    /// Returns the token text for "near" error messages, quoted with `'...'`.
    ///
    /// Matches PUC-Rio's `txtToken` + `LUA_QS`: for Name, Number, and String
    /// tokens, returns the actual content quoted. For other tokens, returns
    /// `token2str()` quoted with `'...'`.
    #[must_use]
    pub fn txt_token(&self) -> String {
        match self {
            Self::Name(s) => format!("'{s}'"),
            Self::Number(n) => format!("'{n}'"),
            Self::Str(s) => {
                let text = String::from_utf8_lossy(s);
                format!("'{text}'")
            }
            _ => format!("'{}'", self.token2str()),
        }
    }

    /// Returns `true` if this token is a block-closing keyword.
    ///
    /// Block followers terminate statement lists. Used by the parser
    /// to detect the end of a block.
    #[must_use]
    pub fn is_block_follow(&self) -> bool {
        matches!(
            self,
            Self::Else | Self::ElseIf | Self::End | Self::Until | Self::Eos
        )
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::And => write!(f, "and"),
            Self::Break => write!(f, "break"),
            Self::Do => write!(f, "do"),
            Self::Else => write!(f, "else"),
            Self::ElseIf => write!(f, "elseif"),
            Self::End => write!(f, "end"),
            Self::False => write!(f, "false"),
            Self::For => write!(f, "for"),
            Self::Function => write!(f, "function"),
            Self::If => write!(f, "if"),
            Self::In => write!(f, "in"),
            Self::Local => write!(f, "local"),
            Self::Nil => write!(f, "nil"),
            Self::Not => write!(f, "not"),
            Self::Or => write!(f, "or"),
            Self::Repeat => write!(f, "repeat"),
            Self::Return => write!(f, "return"),
            Self::Then => write!(f, "then"),
            Self::True => write!(f, "true"),
            Self::Until => write!(f, "until"),
            Self::While => write!(f, "while"),
            Self::Concat => write!(f, ".."),
            Self::Dots => write!(f, "..."),
            Self::Eq => write!(f, "=="),
            Self::Ge => write!(f, ">="),
            Self::Le => write!(f, "<="),
            Self::Ne => write!(f, "~="),
            Self::Number(n) => write!(f, "{n}"),
            Self::Name(s) => write!(f, "{s}"),
            Self::Str(s) => {
                let lossy = String::from_utf8_lossy(s);
                write!(f, "{lossy}")
            }
            Self::Char(c) => write!(f, "{}", char::from(*c)),
            Self::Eos => write!(f, "<eof>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_construction() {
        let span = Span::new(1, 5);
        assert_eq!(span.line, 1);
        assert_eq!(span.column, 5);
    }

    #[test]
    fn reserved_word_display_names() {
        assert_eq!(Token::And.display_name(), "'and'");
        assert_eq!(Token::Function.display_name(), "'function'");
        assert_eq!(Token::While.display_name(), "'while'");
    }

    #[test]
    fn operator_display_names() {
        assert_eq!(Token::Concat.display_name(), "'..'");
        assert_eq!(Token::Dots.display_name(), "'...'");
        assert_eq!(Token::Eq.display_name(), "'=='");
        assert_eq!(Token::Ge.display_name(), "'>='");
        assert_eq!(Token::Le.display_name(), "'<='");
        assert_eq!(Token::Ne.display_name(), "'~='");
    }

    #[test]
    fn literal_display_names() {
        // display_name wraps with '...' (LUA_QS)
        assert_eq!(Token::Number(3.0).display_name(), "'<number>'");
        assert_eq!(Token::Name("foo".into()).display_name(), "'<name>'");
        assert_eq!(Token::Str(b"hello".to_vec()).display_name(), "'<string>'");
    }

    #[test]
    fn char_display_names() {
        assert_eq!(Token::Char(b'+').display_name(), "'+'");
        assert_eq!(Token::Char(b'(').display_name(), "'('");
        assert_eq!(Token::Char(b'=').display_name(), "'='");
    }

    #[test]
    fn eos_display_name() {
        // display_name wraps <eof> with '...' (LUA_QS)
        assert_eq!(Token::Eos.display_name(), "'<eof>'");
    }

    #[test]
    fn token2str_unquoted() {
        assert_eq!(Token::And.token2str(), "and");
        assert_eq!(Token::Eos.token2str(), "<eof>");
        assert_eq!(Token::Char(b'+').token2str(), "+");
        assert_eq!(Token::Concat.token2str(), "..");
    }

    #[test]
    fn token_display() {
        assert_eq!(format!("{}", Token::And), "and");
        assert_eq!(format!("{}", Token::Concat), "..");
        assert_eq!(format!("{}", Token::Number(42.0)), "42");
        assert_eq!(format!("{}", Token::Name("x".into())), "x");
        assert_eq!(format!("{}", Token::Char(b'+')), "+");
        assert_eq!(format!("{}", Token::Eos), "<eof>");
    }

    #[test]
    fn block_follow_tokens() {
        assert!(Token::Else.is_block_follow());
        assert!(Token::ElseIf.is_block_follow());
        assert!(Token::End.is_block_follow());
        assert!(Token::Until.is_block_follow());
        assert!(Token::Eos.is_block_follow());
        assert!(!Token::And.is_block_follow());
        assert!(!Token::Name("x".into()).is_block_follow());
        assert!(!Token::Char(b'+').is_block_follow());
    }

    #[test]
    fn reserved_words_count() {
        assert_eq!(RESERVED_WORDS.len(), 21);
    }

    #[test]
    fn lookup_keyword_matches_reserved_words() {
        for &(word, ref expected) in RESERVED_WORDS {
            let result = lookup_keyword(word.as_bytes());
            assert_eq!(
                result.as_ref(),
                Some(expected),
                "lookup_keyword({word:?}) failed"
            );
        }
        // Non-keywords return None.
        assert_eq!(lookup_keyword(b"foo"), None);
        assert_eq!(lookup_keyword(b"And"), None);
        assert_eq!(lookup_keyword(b"IF"), None);
        assert_eq!(lookup_keyword(b""), None);
    }

    #[test]
    fn reserved_words_sorted() {
        for window in RESERVED_WORDS.windows(2) {
            assert!(
                window[0].0 < window[1].0,
                "not sorted: {} >= {}",
                window[0].0,
                window[1].0
            );
        }
    }

    #[test]
    fn token_equality() {
        assert_eq!(Token::And, Token::And);
        assert_eq!(Token::Number(1.0), Token::Number(1.0));
        assert_ne!(Token::Number(1.0), Token::Number(2.0));
        assert_eq!(Token::Name("x".into()), Token::Name("x".into()));
        assert_ne!(Token::Name("x".into()), Token::Name("y".into()));
    }
}
