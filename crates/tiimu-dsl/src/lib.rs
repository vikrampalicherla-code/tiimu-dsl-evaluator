//!
//! Parser for the TIIMU Text DSL.
//!
//! Parses a human-friendly Boolean expression language into the shared AST in `tiimu-expr-ast`.
//!
//! Typical pipeline:
//! 1. Author writes DSL text (e.g. in a UI builder).
//! 2. Parse DSL -> AST.
//! 3. Deploy-time validate AST against a dictionary snapshot + function registry.
//! 4. Store DSL + AST JSON + dependencies.
//! 5. Runtime evaluates AST against a small context map (deterministic, no UNKNOWN).

use pest::Parser;
use pest_derive::Parser;
use thiserror::Error;
use tiimu_expr_ast::{CompareOp, ContainsOp, Expr, FieldRef, Literal, LiteralOrField, LogicalOp, MembershipOp};

#[derive(Parser)]
#[grammar = "expr.pest"]
struct ExprParser;

#[derive(Debug, Error)]
pub enum DslError {
    #[error("parse error: {0}")]
    Parse(String),
}

/// Parses DSL text into an AST (`tiimu_expr_ast::Expr`).
///
/// Returns a structured parse error string if the input is invalid.
pub fn parse_expression(input: &str) -> Result<Expr, DslError> {
    let mut pairs = ExprParser::parse(Rule::expression, input).map_err(|e| DslError::Parse(e.to_string()))?;
    let pair = pairs.next().ok_or_else(|| DslError::Parse("empty expression".into()))?;
    build_expr(pair)
}

fn build_expr(pair: pest::iterators::Pair<Rule>) -> Result<Expr, DslError> {
    match pair.as_rule() {
        Rule::expression => build_expr(pair.into_inner().next().unwrap()),
        Rule::or_expr => {
            let mut inner = pair.into_inner();
            let mut expr = build_expr(inner.next().unwrap())?;
            while let Some(_) = inner.next() {
                let rhs = build_expr(inner.next().unwrap())?;
                expr = Expr::Logical { op: LogicalOp::Or, lhs: Box::new(expr), rhs: Box::new(rhs) };
            }
            Ok(expr)
        }
        Rule::and_expr => {
            let mut inner = pair.into_inner();
            let mut expr = build_expr(inner.next().unwrap())?;
            while let Some(_) = inner.next() {
                let rhs = build_expr(inner.next().unwrap())?;
                expr = Expr::Logical { op: LogicalOp::And, lhs: Box::new(expr), rhs: Box::new(rhs) };
            }
            Ok(expr)
        }
        Rule::unary_expr => {
            let s = pair.as_str().trim();
            let mut inner = pair.into_inner();
            let prim = inner.next_back().unwrap();
            let e = build_expr(prim)?;
            if s.starts_with('!') { Ok(Expr::Not(Box::new(e))) } else { Ok(e) }
        }
        Rule::primary => build_expr(pair.into_inner().next().unwrap()),
        Rule::predicate => build_predicate(pair),
        Rule::function_call => build_call(pair),
        Rule::literal => Ok(Expr::Literal(build_literal(pair.into_inner().next().unwrap())?)),
        Rule::field_expr => Ok(Expr::Field(parse_field_ref(pair.as_str()))),
        _ => Ok(Expr::Literal(Literal::Null)),
    }
}

fn parse_field_ref(s: &str) -> FieldRef {
    FieldRef::new(s.split('.').map(|x| x.to_string()).collect())
}

fn build_literal(pair: pest::iterators::Pair<Rule>) -> Result<Literal, DslError> {
    match pair.as_rule() {
        Rule::boolean => Ok(Literal::Bool(pair.as_str() == "true")),
        Rule::number => Ok(Literal::Number(pair.as_str().parse().map_err(|_| DslError::Parse("invalid number".into()))?)),
        Rule::string => {
            let raw = pair.as_str();
            let inner = &raw[1..raw.len()-1];
            Ok(Literal::String(inner.replace("\\\"", "\"").replace("\\\\", "\\")))
        }
        Rule::null => Ok(Literal::Null),
        _ => Ok(Literal::Null),
    }
}

fn build_value_or_field(pair: pest::iterators::Pair<Rule>) -> Result<LiteralOrField, DslError> {
    match pair.as_rule() {
        Rule::value => build_value_or_field(pair.into_inner().next().unwrap()),
        Rule::field_ref => Ok(LiteralOrField::Field(parse_field_ref(pair.as_str()))),
        Rule::string | Rule::number | Rule::boolean | Rule::null => Ok(LiteralOrField::Lit(build_literal(pair)?)),
        _ => Ok(LiteralOrField::Lit(Literal::Null)),
    }
}

fn build_predicate(pair: pest::iterators::Pair<Rule>) -> Result<Expr, DslError> {
    let text = pair.as_str();
    let mut inner = pair.into_inner();

    let field = parse_field_ref(inner.next().unwrap().as_str());
    let rest: Vec<_> = inner.collect();

    if text.contains(" contains ") {
        let value = build_value_or_field(rest.last().unwrap().clone())?;
        Ok(Expr::Contains { field, op: ContainsOp::Contains, value })
    } else if text.contains("~") {
        let regex_pair = rest.iter().find(|p| p.as_rule() == Rule::regex).unwrap();
        let raw = regex_pair.as_str();
        Ok(Expr::RegexMatch { field, pattern: raw[1..raw.len()-1].to_string() })
    } else if text.contains(" in ") || text.contains(" not in ") {
        let op = if text.contains(" not in ") { MembershipOp::NotIn } else { MembershipOp::In };
        let target = rest.last().unwrap().clone();
        let list = match target.as_rule() {
            Rule::list => {
                let mut items = vec![];
                for p in target.into_inner() {
                    if p.as_rule() == Rule::value {
                        items.push(build_value_or_field(p)?);
                    }
                }
                LiteralOrField::Lit(Literal::List(items))
            }
            Rule::field_ref => LiteralOrField::Field(parse_field_ref(target.as_str())),
            _ => LiteralOrField::Lit(Literal::Null),
        };
        Ok(Expr::Membership { field, op, list })
    } else {
        let comp_pair = rest.iter().find(|p| p.as_rule() == Rule::comparator).unwrap();
        let op = match comp_pair.as_str() {
            "==" => CompareOp::Eq, "!=" => CompareOp::Ne,
            "<" => CompareOp::Lt, "<=" => CompareOp::Le,
            ">" => CompareOp::Gt, ">=" => CompareOp::Ge,
            _ => CompareOp::Eq,
        };
        let value = build_value_or_field(rest.last().unwrap().clone())?;
        Ok(Expr::Compare { field, op, value })
    }
}

fn build_call(pair: pest::iterators::Pair<Rule>) -> Result<Expr, DslError> {
    let mut inner = pair.into_inner();
    let name = inner.next().unwrap().as_str().to_string();
    let args = inner.filter(|p| matches!(p.as_rule(), Rule::expression | Rule::or_expr))
        .map(build_expr)
        .collect::<Result<Vec<_>,_>>()?;
    Ok(Expr::Call { name, args })
}
