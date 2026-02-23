//! RQL Lexer/Tokenizer
//!
//! Converts an RQL query string into a stream of tokens.

use super::error::{LexerError, RqlError, RqlResult};

/// Token types for RQL.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    Select,
    From,
    Where,
    Update,
    Delete,
    Set,
    And,
    Or,
    Not,
    In,
    Like,
    Is,
    Null,
    Contains,
    All,
    Any,
    Search,
    Reason,
    With,
    Confidence,
    Order,
    Group,
    By,
    Asc,
    Desc,
    Limit,
    Offset,
    As,
    Explain,
    // Relationship keywords
    Related,
    To,
    References,
    ReferencedBy,
    FollowsUp,
    FollowedUpBy,
    Supersedes,
    SupersededBy,
    ParentOf,
    ChildOf,
    // Aggregate functions
    Count,
    Sum,
    Avg,
    Min,
    Max,
    // Boolean literals
    True,
    False,

    // Symbols
    Star,     // *
    Comma,    // ,
    Dot,      // .
    LParen,   // (
    RParen,   // )
    LBracket, // [
    RBracket, // ]

    // Operators
    Eq, // =
    Ne, // != or <>
    Lt, // <
    Gt, // >
    Le, // <=
    Ge, // >=

    // Literals
    String(String),
    Number(f64),
    Identifier(String),

    // End of input
    Eof,
}

impl Token {
    /// Check if this token is a keyword.
    #[allow(dead_code)]
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Token::Select
                | Token::From
                | Token::Where
                | Token::Update
                | Token::Delete
                | Token::Set
                | Token::And
                | Token::Or
                | Token::Not
                | Token::In
                | Token::Like
                | Token::Is
                | Token::Null
                | Token::Contains
                | Token::All
                | Token::Any
                | Token::Search
                | Token::Reason
                | Token::With
                | Token::Confidence
                | Token::Order
                | Token::Group
                | Token::By
                | Token::Asc
                | Token::Desc
                | Token::Limit
                | Token::Offset
                | Token::As
                | Token::Explain
                | Token::Related
                | Token::To
                | Token::References
                | Token::ReferencedBy
                | Token::FollowsUp
                | Token::FollowedUpBy
                | Token::Supersedes
                | Token::SupersededBy
                | Token::ParentOf
                | Token::ChildOf
                | Token::Count
                | Token::Sum
                | Token::Avg
                | Token::Min
                | Token::Max
                | Token::True
                | Token::False
        )
    }
}

/// Lexer for RQL queries.
pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    /// Create a new lexer for the given input.
    pub fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }

    /// Tokenize the entire input.
    pub fn tokenize(&mut self) -> RqlResult<Vec<Token>> {
        let mut tokens = Vec::new();

        loop {
            let token = self.next_token()?;
            let is_eof = token == Token::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        Ok(tokens)
    }

    /// Get the next token.
    fn next_token(&mut self) -> RqlResult<Token> {
        self.skip_whitespace();

        if self.pos >= self.input.len() {
            return Ok(Token::Eof);
        }

        let ch = self.current();

        // String literals
        if ch == '\'' || ch == '"' {
            return self.read_string(ch);
        }

        // Numbers
        if ch.is_ascii_digit() || (ch == '-' && self.peek().is_some_and(|c| c.is_ascii_digit())) {
            return self.read_number();
        }

        // Identifiers and keywords
        if ch.is_ascii_alphabetic() || ch == '_' {
            return self.read_identifier();
        }

        // Operators and symbols
        match ch {
            '*' => {
                self.advance();
                Ok(Token::Star)
            }
            ',' => {
                self.advance();
                Ok(Token::Comma)
            }
            '.' => {
                self.advance();
                Ok(Token::Dot)
            }
            '(' => {
                self.advance();
                Ok(Token::LParen)
            }
            ')' => {
                self.advance();
                Ok(Token::RParen)
            }
            '[' => {
                self.advance();
                Ok(Token::LBracket)
            }
            ']' => {
                self.advance();
                Ok(Token::RBracket)
            }
            '=' => {
                self.advance();
                Ok(Token::Eq)
            }
            '!' => {
                self.advance();
                if self.current() == '=' {
                    self.advance();
                    Ok(Token::Ne)
                } else {
                    Err(self.error("Expected '=' after '!'", "!"))
                }
            }
            '<' => {
                self.advance();
                if self.current() == '=' {
                    self.advance();
                    Ok(Token::Le)
                } else if self.current() == '>' {
                    self.advance();
                    Ok(Token::Ne)
                } else {
                    Ok(Token::Lt)
                }
            }
            '>' => {
                self.advance();
                if self.current() == '=' {
                    self.advance();
                    Ok(Token::Ge)
                } else {
                    Ok(Token::Gt)
                }
            }
            _ => Err(self.error(&format!("Unexpected character '{}'", ch), &ch.to_string())),
        }
    }

    /// Read a string literal.
    fn read_string(&mut self, quote: char) -> RqlResult<Token> {
        self.advance(); // Skip opening quote
        let mut value = String::new();

        while self.pos < self.input.len() {
            let ch = self.current();

            if ch == quote {
                self.advance(); // Skip closing quote
                return Ok(Token::String(value));
            }

            if ch == '\\' {
                self.advance();
                if self.pos < self.input.len() {
                    let escaped = match self.current() {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '\\' => '\\',
                        c if c == quote => quote,
                        c => c,
                    };
                    value.push(escaped);
                    self.advance();
                }
            } else {
                value.push(ch);
                self.advance();
            }
        }

        Err(self.error("Unterminated string literal", &value))
    }

    /// Read a numeric literal.
    fn read_number(&mut self) -> RqlResult<Token> {
        let mut value = String::new();

        // Handle negative sign
        if self.current() == '-' {
            value.push('-');
            self.advance();
        }

        // Integer part
        while self.pos < self.input.len() && self.current().is_ascii_digit() {
            value.push(self.current());
            self.advance();
        }

        // Decimal part
        if self.pos < self.input.len() && self.current() == '.' {
            value.push('.');
            self.advance();
            while self.pos < self.input.len() && self.current().is_ascii_digit() {
                value.push(self.current());
                self.advance();
            }
        }

        value
            .parse::<f64>()
            .map(Token::Number)
            .map_err(|_| self.error("Invalid number", &value))
    }

    /// Read an identifier or keyword.
    fn read_identifier(&mut self) -> RqlResult<Token> {
        let mut value = String::new();

        while self.pos < self.input.len() {
            let ch = self.current();
            if ch.is_ascii_alphanumeric() || ch == '_' {
                value.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        // Check for keywords (case-insensitive)
        let token = match value.to_uppercase().as_str() {
            "SELECT" => Token::Select,
            "FROM" => Token::From,
            "WHERE" => Token::Where,
            "UPDATE" => Token::Update,
            "DELETE" => Token::Delete,
            "SET" => Token::Set,
            "AND" => Token::And,
            "OR" => Token::Or,
            "NOT" => Token::Not,
            "IN" => Token::In,
            "LIKE" => Token::Like,
            "IS" => Token::Is,
            "NULL" => Token::Null,
            "CONTAINS" => Token::Contains,
            "ALL" => Token::All,
            "ANY" => Token::Any,
            "SEARCH" => Token::Search,
            "REASON" => Token::Reason,
            "WITH" => Token::With,
            "CONFIDENCE" => Token::Confidence,
            "ORDER" => Token::Order,
            "GROUP" => Token::Group,
            "BY" => Token::By,
            "ASC" => Token::Asc,
            "DESC" => Token::Desc,
            "LIMIT" => Token::Limit,
            "OFFSET" => Token::Offset,
            "AS" => Token::As,
            "EXPLAIN" => Token::Explain,
            // Relationship keywords
            "RELATED" => Token::Related,
            "TO" => Token::To,
            "REFERENCES" => Token::References,
            "REFERENCED_BY" => Token::ReferencedBy,
            "FOLLOWS_UP" => Token::FollowsUp,
            "FOLLOWED_UP_BY" => Token::FollowedUpBy,
            "SUPERSEDES" => Token::Supersedes,
            "SUPERSEDED_BY" => Token::SupersededBy,
            "PARENT_OF" => Token::ParentOf,
            "CHILD_OF" => Token::ChildOf,
            // Aggregate functions
            "COUNT" => Token::Count,
            "SUM" => Token::Sum,
            "AVG" => Token::Avg,
            "MIN" => Token::Min,
            "MAX" => Token::Max,
            "TRUE" => Token::True,
            "FALSE" => Token::False,
            _ => Token::Identifier(value),
        };

        Ok(token)
    }

    /// Skip whitespace and comments.
    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() {
            let ch = self.current();

            if ch.is_whitespace() {
                if ch == '\n' {
                    self.line += 1;
                    self.column = 1;
                } else {
                    self.column += 1;
                }
                self.pos += 1;
            } else if ch == '-' && self.peek() == Some('-') {
                // SQL-style comment: -- ...
                while self.pos < self.input.len() && self.current() != '\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    /// Get the current character.
    fn current(&self) -> char {
        self.input.get(self.pos).copied().unwrap_or('\0')
    }

    /// Peek at the next character.
    fn peek(&self) -> Option<char> {
        self.input.get(self.pos + 1).copied()
    }

    /// Advance to the next character.
    fn advance(&mut self) {
        self.pos += 1;
        self.column += 1;
    }

    /// Create a lexer error.
    fn error(&self, message: &str, found: &str) -> RqlError {
        LexerError {
            message: message.to_string(),
            line: self.line,
            column: self.column,
            found: found.to_string(),
        }
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_select() {
        let mut lexer = Lexer::new("SELECT * FROM test");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Select,
                Token::Star,
                Token::From,
                Token::Identifier("test".to_string()),
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_where_clause() {
        let mut lexer = Lexer::new("SELECT * FROM t WHERE x = 'hello'");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Select,
                Token::Star,
                Token::From,
                Token::Identifier("t".to_string()),
                Token::Where,
                Token::Identifier("x".to_string()),
                Token::Eq,
                Token::String("hello".to_string()),
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_numbers() {
        let mut lexer = Lexer::new("WHERE x > 100 AND y < 3.15");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Where,
                Token::Identifier("x".to_string()),
                Token::Gt,
                Token::Number(100.0),
                Token::And,
                Token::Identifier("y".to_string()),
                Token::Lt,
                Token::Number(3.15),
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_operators() {
        let mut lexer = Lexer::new("= != < > <= >= <>");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Eq,
                Token::Ne,
                Token::Lt,
                Token::Gt,
                Token::Le,
                Token::Ge,
                Token::Ne,
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_keywords_case_insensitive() {
        let mut lexer = Lexer::new("select FROM where AND or NOT");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Select,
                Token::From,
                Token::Where,
                Token::And,
                Token::Or,
                Token::Not,
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_field_path() {
        let mut lexer = Lexer::new("metadata.status");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Identifier("metadata".to_string()),
                Token::Dot,
                Token::Identifier("status".to_string()),
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_array_access() {
        let mut lexer = Lexer::new("tags[0]");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Identifier("tags".to_string()),
                Token::LBracket,
                Token::Number(0.0),
                Token::RBracket,
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_update_tokens() {
        let mut lexer = Lexer::new("UPDATE t SET x = 1");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Update,
                Token::Identifier("t".to_string()),
                Token::Set,
                Token::Identifier("x".to_string()),
                Token::Eq,
                Token::Number(1.0),
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_delete_tokens() {
        let mut lexer = Lexer::new("DELETE FROM t WHERE x = 1");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Delete,
                Token::From,
                Token::Identifier("t".to_string()),
                Token::Where,
                Token::Identifier("x".to_string()),
                Token::Eq,
                Token::Number(1.0),
                Token::Eof
            ]
        );
    }
}
