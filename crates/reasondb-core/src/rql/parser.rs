//! RQL Parser
//!
//! Converts a stream of tokens into an Abstract Syntax Tree (AST).

use super::ast::*;
use super::error::{ParserError, RqlResult};
use super::lexer::Token;

/// Parser for RQL queries.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    /// Create a new parser for the given tokens.
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    /// Parse the tokens into a Query AST.
    pub fn parse(&mut self) -> RqlResult<Query> {
        let select = self.parse_select()?;
        let from = self.parse_from()?;
        let where_clause = self.parse_where()?;
        let search = self.parse_search_clause()?;
        let reason = self.parse_reason_clause()?;
        let order_by = self.parse_order_by()?;
        let limit = self.parse_limit()?;

        // Should be at EOF
        if !self.is_at_end() {
            return Err(ParserError::new("Unexpected tokens after query")
                .found(format!("{:?}", self.current()))
                .into());
        }

        Ok(Query {
            select,
            from,
            where_clause,
            search,
            reason,
            order_by,
            limit,
        })
    }

    // ==================== SELECT ====================

    fn parse_select(&mut self) -> RqlResult<SelectClause> {
        self.expect(Token::Select)?;

        if self.check(&Token::Star) {
            self.advance();
            return Ok(SelectClause::All);
        }

        if self.check(&Token::Count) {
            self.advance();
            self.expect(Token::LParen)?;
            self.expect(Token::Star)?;
            self.expect(Token::RParen)?;
            return Ok(SelectClause::Count);
        }

        // Parse field list
        let mut fields = Vec::new();
        loop {
            let path = self.parse_field_path()?;
            let alias = if self.check(&Token::As) {
                self.advance();
                Some(self.parse_identifier()?)
            } else {
                None
            };
            fields.push(FieldSelector { path, alias });

            if !self.check(&Token::Comma) {
                break;
            }
            self.advance(); // consume comma
        }

        Ok(SelectClause::Fields(fields))
    }

    // ==================== FROM ====================

    fn parse_from(&mut self) -> RqlResult<FromClause> {
        self.expect(Token::From)?;
        let table = self.parse_identifier()?;
        Ok(FromClause { table })
    }

    // ==================== WHERE ====================

    fn parse_where(&mut self) -> RqlResult<Option<WhereClause>> {
        if !self.check(&Token::Where) {
            return Ok(None);
        }
        self.advance();

        let condition = self.parse_or_condition()?;
        Ok(Some(WhereClause { condition }))
    }

    fn parse_or_condition(&mut self) -> RqlResult<Condition> {
        let mut left = self.parse_and_condition()?;

        while self.check(&Token::Or) {
            self.advance();
            let right = self.parse_and_condition()?;
            left = Condition::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_and_condition(&mut self) -> RqlResult<Condition> {
        let mut left = self.parse_not_condition()?;

        while self.check(&Token::And) {
            self.advance();
            let right = self.parse_not_condition()?;
            left = Condition::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_not_condition(&mut self) -> RqlResult<Condition> {
        if self.check(&Token::Not) {
            self.advance();
            let inner = self.parse_primary_condition()?;
            return Ok(Condition::Not(Box::new(inner)));
        }

        self.parse_primary_condition()
    }

    fn parse_primary_condition(&mut self) -> RqlResult<Condition> {
        // Parenthesized condition
        if self.check(&Token::LParen) {
            self.advance();
            let cond = self.parse_or_condition()?;
            self.expect(Token::RParen)?;
            return Ok(cond);
        }

        // Check for 'value' IN field pattern
        if let Token::String(s) = self.current().clone() {
            let next_pos = self.pos + 1;
            if next_pos < self.tokens.len() && self.tokens[next_pos] == Token::In {
                self.advance(); // consume string
                self.advance(); // consume IN
                let field = self.parse_field_path()?;
                return Ok(Condition::Comparison(Comparison {
                    left: field,
                    operator: ComparisonOp::ContainsAny,
                    right: Value::Array(vec![Value::String(s)]),
                }));
            }
        }

        // Regular comparison: field op value
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> RqlResult<Condition> {
        let left = self.parse_field_path()?;

        // IS NULL / IS NOT NULL
        if self.check(&Token::Is) {
            self.advance();
            if self.check(&Token::Not) {
                self.advance();
                self.expect(Token::Null)?;
                return Ok(Condition::Comparison(Comparison {
                    left,
                    operator: ComparisonOp::IsNotNull,
                    right: Value::Null,
                }));
            }
            self.expect(Token::Null)?;
            return Ok(Condition::Comparison(Comparison {
                left,
                operator: ComparisonOp::IsNull,
                right: Value::Null,
            }));
        }

        // CONTAINS ALL/ANY
        if self.check(&Token::Contains) {
            self.advance();
            let op = if self.check(&Token::All) {
                self.advance();
                ComparisonOp::ContainsAll
            } else if self.check(&Token::Any) {
                self.advance();
                ComparisonOp::ContainsAny
            } else {
                return Err(ParserError::new("Expected ALL or ANY after CONTAINS")
                    .expected("ALL or ANY")
                    .into());
            };
            let values = self.parse_value_list()?;
            return Ok(Condition::Comparison(Comparison {
                left,
                operator: op,
                right: Value::Array(values),
            }));
        }

        // IN (values)
        if self.check(&Token::In) {
            self.advance();
            let values = self.parse_value_list()?;
            return Ok(Condition::Comparison(Comparison {
                left,
                operator: ComparisonOp::In,
                right: Value::Array(values),
            }));
        }

        // LIKE
        if self.check(&Token::Like) {
            self.advance();
            let pattern = self.parse_value()?;
            return Ok(Condition::Comparison(Comparison {
                left,
                operator: ComparisonOp::Like,
                right: pattern,
            }));
        }

        // Standard comparison operators
        let operator = self.parse_comparison_op()?;
        let right = self.parse_value()?;

        Ok(Condition::Comparison(Comparison {
            left,
            operator,
            right,
        }))
    }

    fn parse_comparison_op(&mut self) -> RqlResult<ComparisonOp> {
        let op = match self.current() {
            Token::Eq => ComparisonOp::Eq,
            Token::Ne => ComparisonOp::Ne,
            Token::Lt => ComparisonOp::Lt,
            Token::Gt => ComparisonOp::Gt,
            Token::Le => ComparisonOp::Le,
            Token::Ge => ComparisonOp::Ge,
            _ => {
                return Err(ParserError::new("Expected comparison operator")
                    .expected("=, !=, <, >, <=, >=")
                    .found(format!("{:?}", self.current()))
                    .into())
            }
        };
        self.advance();
        Ok(op)
    }

    // ==================== SEARCH ====================

    fn parse_search_clause(&mut self) -> RqlResult<Option<SearchClause>> {
        if self.check(&Token::Search) {
            self.advance();
            let query = self.parse_string()?;
            return Ok(Some(SearchClause { query }));
        }
        Ok(None)
    }

    // ==================== REASON ====================

    fn parse_reason_clause(&mut self) -> RqlResult<Option<ReasonClause>> {
        if self.check(&Token::Reason) {
            self.advance();
            let query = self.parse_string()?;

            // Optional: WITH CONFIDENCE > 0.8
            let min_confidence = if self.check(&Token::With) {
                self.advance();
                self.expect(Token::Confidence)?;
                self.expect(Token::Gt)?;
                let num = self.parse_number()?;
                Some(num as f32)
            } else {
                None
            };

            return Ok(Some(ReasonClause {
                query,
                min_confidence,
            }));
        }
        Ok(None)
    }

    // ==================== ORDER BY ====================

    fn parse_order_by(&mut self) -> RqlResult<Option<OrderByClause>> {
        if !self.check(&Token::Order) {
            return Ok(None);
        }
        self.advance();
        self.expect(Token::By)?;

        let field = self.parse_field_path()?;
        let direction = if self.check(&Token::Desc) {
            self.advance();
            SortDirection::Desc
        } else if self.check(&Token::Asc) {
            self.advance();
            SortDirection::Asc
        } else {
            SortDirection::Asc
        };

        Ok(Some(OrderByClause { field, direction }))
    }

    // ==================== LIMIT ====================

    fn parse_limit(&mut self) -> RqlResult<Option<LimitClause>> {
        if !self.check(&Token::Limit) {
            return Ok(None);
        }
        self.advance();

        let count = self.parse_number()? as usize;
        let offset = if self.check(&Token::Offset) {
            self.advance();
            Some(self.parse_number()? as usize)
        } else {
            None
        };

        Ok(Some(LimitClause { count, offset }))
    }

    // ==================== Helpers ====================

    fn parse_field_path(&mut self) -> RqlResult<FieldPath> {
        let mut segments = Vec::new();

        // First segment must be identifier
        let first = self.parse_identifier()?;
        segments.push(PathSegment::Field(first));

        // Additional segments
        loop {
            if self.check(&Token::Dot) {
                self.advance();
                let name = self.parse_identifier()?;
                segments.push(PathSegment::Field(name));
            } else if self.check(&Token::LBracket) {
                self.advance();
                let idx = self.parse_number()? as usize;
                self.expect(Token::RBracket)?;
                segments.push(PathSegment::Index(idx));
            } else {
                break;
            }
        }

        Ok(FieldPath { segments })
    }

    fn parse_value(&mut self) -> RqlResult<Value> {
        match self.current().clone() {
            Token::String(s) => {
                self.advance();
                Ok(Value::String(s))
            }
            Token::Number(n) => {
                self.advance();
                Ok(Value::Number(n))
            }
            Token::True => {
                self.advance();
                Ok(Value::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(Value::Bool(false))
            }
            Token::Null => {
                self.advance();
                Ok(Value::Null)
            }
            _ => Err(ParserError::new("Expected value")
                .expected("string, number, true, false, or null")
                .found(format!("{:?}", self.current()))
                .into()),
        }
    }

    fn parse_value_list(&mut self) -> RqlResult<Vec<Value>> {
        self.expect(Token::LParen)?;
        let mut values = Vec::new();

        if !self.check(&Token::RParen) {
            loop {
                values.push(self.parse_value()?);
                if !self.check(&Token::Comma) {
                    break;
                }
                self.advance();
            }
        }

        self.expect(Token::RParen)?;
        Ok(values)
    }

    fn parse_identifier(&mut self) -> RqlResult<String> {
        match self.current().clone() {
            Token::Identifier(s) => {
                self.advance();
                Ok(s)
            }
            _ => Err(ParserError::new("Expected identifier")
                .found(format!("{:?}", self.current()))
                .into()),
        }
    }

    fn parse_string(&mut self) -> RqlResult<String> {
        match self.current().clone() {
            Token::String(s) => {
                self.advance();
                Ok(s)
            }
            _ => Err(ParserError::new("Expected string")
                .found(format!("{:?}", self.current()))
                .into()),
        }
    }

    fn parse_number(&mut self) -> RqlResult<f64> {
        match self.current().clone() {
            Token::Number(n) => {
                self.advance();
                Ok(n)
            }
            _ => Err(ParserError::new("Expected number")
                .found(format!("{:?}", self.current()))
                .into()),
        }
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn check(&self, token: &Token) -> bool {
        std::mem::discriminant(self.current()) == std::mem::discriminant(token)
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn expect(&mut self, token: Token) -> RqlResult<()> {
        if self.check(&token) {
            self.advance();
            Ok(())
        } else {
            Err(ParserError::new(format!("Expected {:?}", token))
                .expected(format!("{:?}", token))
                .found(format!("{:?}", self.current()))
                .into())
        }
    }

    fn is_at_end(&self) -> bool {
        matches!(self.current(), Token::Eof)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rql::lexer::Lexer;

    fn parse(input: &str) -> RqlResult<Query> {
        let tokens = Lexer::new(input).tokenize()?;
        Parser::new(tokens).parse()
    }

    #[test]
    fn test_simple_select() {
        let query = parse("SELECT * FROM test").unwrap();
        assert_eq!(query.select, SelectClause::All);
        assert_eq!(query.from.table, "test");
        assert!(query.where_clause.is_none());
    }

    #[test]
    fn test_select_fields() {
        let query = parse("SELECT title, author FROM docs").unwrap();
        match query.select {
            SelectClause::Fields(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].path.to_string(), "title");
                assert_eq!(fields[1].path.to_string(), "author");
            }
            _ => panic!("Expected Fields"),
        }
    }

    #[test]
    fn test_where_equals() {
        let query = parse("SELECT * FROM t WHERE status = 'active'").unwrap();
        let where_clause = query.where_clause.unwrap();
        match where_clause.condition {
            Condition::Comparison(comp) => {
                assert_eq!(comp.left.to_string(), "status");
                assert_eq!(comp.operator, ComparisonOp::Eq);
                assert_eq!(comp.right, Value::String("active".to_string()));
            }
            _ => panic!("Expected comparison"),
        }
    }

    #[test]
    fn test_where_and() {
        let query = parse("SELECT * FROM t WHERE a = 1 AND b = 2").unwrap();
        let where_clause = query.where_clause.unwrap();
        match where_clause.condition {
            Condition::And(_, _) => {}
            _ => panic!("Expected AND condition"),
        }
    }

    #[test]
    fn test_where_or() {
        let query = parse("SELECT * FROM t WHERE a = 1 OR b = 2").unwrap();
        let where_clause = query.where_clause.unwrap();
        match where_clause.condition {
            Condition::Or(_, _) => {}
            _ => panic!("Expected OR condition"),
        }
    }

    #[test]
    fn test_nested_field() {
        let query = parse("SELECT * FROM t WHERE metadata.status = 'active'").unwrap();
        let where_clause = query.where_clause.unwrap();
        match where_clause.condition {
            Condition::Comparison(comp) => {
                assert_eq!(comp.left.to_string(), "metadata.status");
            }
            _ => panic!("Expected comparison"),
        }
    }

    #[test]
    fn test_contains_all() {
        let query = parse("SELECT * FROM t WHERE tags CONTAINS ALL ('a', 'b')").unwrap();
        let where_clause = query.where_clause.unwrap();
        match where_clause.condition {
            Condition::Comparison(comp) => {
                assert_eq!(comp.operator, ComparisonOp::ContainsAll);
            }
            _ => panic!("Expected comparison"),
        }
    }

    #[test]
    fn test_search_clause() {
        let query = parse("SELECT * FROM t SEARCH 'hello world'").unwrap();
        let search = query.search.expect("Expected search clause");
        assert_eq!(search.query, "hello world");
    }

    #[test]
    fn test_reason_clause() {
        let query = parse("SELECT * FROM t REASON 'what is X?' WITH CONFIDENCE > 0.8").unwrap();
        let reason = query.reason.expect("Expected reason clause");
        assert_eq!(reason.query, "what is X?");
        assert_eq!(reason.min_confidence, Some(0.8));
    }

    #[test]
    fn test_combined_search_reason() {
        let query = parse("SELECT * FROM t SEARCH 'payment' REASON 'What are the fees?'").unwrap();
        let search = query.search.expect("Expected search clause");
        assert_eq!(search.query, "payment");
        let reason = query.reason.expect("Expected reason clause");
        assert_eq!(reason.query, "What are the fees?");
    }

    #[test]
    fn test_order_by() {
        let query = parse("SELECT * FROM t ORDER BY created_at DESC").unwrap();
        let order = query.order_by.unwrap();
        assert_eq!(order.field.to_string(), "created_at");
        assert_eq!(order.direction, SortDirection::Desc);
    }

    #[test]
    fn test_limit_offset() {
        let query = parse("SELECT * FROM t LIMIT 10 OFFSET 20").unwrap();
        let limit = query.limit.unwrap();
        assert_eq!(limit.count, 10);
        assert_eq!(limit.offset, Some(20));
    }

    #[test]
    fn test_complex_query() {
        let query = parse(
            "SELECT * FROM legal_contracts \
             WHERE metadata.status = 'active' AND metadata.value > 1000 \
             SEARCH 'indemnification' \
             ORDER BY created_at DESC \
             LIMIT 10",
        )
        .unwrap();

        assert_eq!(query.from.table, "legal_contracts");
        assert!(query.where_clause.is_some());
        assert!(query.search.is_some());
        assert!(query.order_by.is_some());
        assert_eq!(query.limit.unwrap().count, 10);
    }
}
