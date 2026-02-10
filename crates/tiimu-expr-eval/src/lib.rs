use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tiimu_expr_ast::{CompareOp, Expr, FieldRef, Literal, LiteralOrField, LogicalOp, MembershipOp};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Bool(bool),
    Number(f64),
    String(String),
    Null,
    Set(Vec<Value>),
}

#[derive(Debug, Clone)]
pub struct EvalContext {
    pub values: HashMap<String, Value>,
}
impl EvalContext {
    pub fn new(values: HashMap<String, Value>) -> Self { Self { values } }
    pub fn get(&self, field: &FieldRef) -> Option<&Value> { self.values.get(&field.as_dotted()) }
}

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("missing field at runtime: {0}")]
    MissingField(String),
    #[error("type error: {0}")]
    Type(String),
    #[error("regex error: {0}")]
    Regex(String),
}

pub fn eval(expr: &Expr, ctx: &EvalContext) -> Result<bool, EvalError> {
    match eval_value(expr, ctx)? {
        Value::Bool(b) => Ok(b),
        _ => Err(EvalError::Type("top-level must be bool".into())),
    }
}

fn eval_value(expr: &Expr, ctx: &EvalContext) -> Result<Value, EvalError> {
    match expr {
        Expr::Not(e) => Ok(Value::Bool(!as_bool(&eval_value(e, ctx)?)?)),
        Expr::Logical { op, lhs, rhs } => match op {
            LogicalOp::And => {
                let l = as_bool(&eval_value(lhs, ctx)?)?;
                if !l { return Ok(Value::Bool(false)); }
                let r = as_bool(&eval_value(rhs, ctx)?)?;
                Ok(Value::Bool(l && r))
            }
            LogicalOp::Or => {
                let l = as_bool(&eval_value(lhs, ctx)?)?;
                if l { return Ok(Value::Bool(true)); }
                let r = as_bool(&eval_value(rhs, ctx)?)?;
                Ok(Value::Bool(l || r))
            }
        },
        Expr::Compare { field, op, value } => {
            let fv = ctx.get(field).ok_or_else(|| EvalError::MissingField(field.as_dotted()))?.clone();
            let vv = eval_lit_or_field(value, ctx)?;
            Ok(Value::Bool(compare(op, &fv, &vv)?))
        }
        Expr::Membership { field, op, list } => {
            let fv = ctx.get(field).ok_or_else(|| EvalError::MissingField(field.as_dotted()))?.clone();
            let target = eval_lit_or_field(list, ctx)?;
            Ok(Value::Bool(membership(op, &fv, &target)?))
        }
        Expr::Contains { field, value, .. } => {
            let fv = ctx.get(field).ok_or_else(|| EvalError::MissingField(field.as_dotted()))?.clone();
            let vv = eval_lit_or_field(value, ctx)?;
            Ok(Value::Bool(contains(&fv, &vv)?))
        }
        Expr::RegexMatch { field, pattern } => {
            let fv = ctx.get(field).ok_or_else(|| EvalError::MissingField(field.as_dotted()))?.clone();
            let s = as_string(&fv)?;
            let re = Regex::new(pattern).map_err(|e| EvalError::Regex(e.to_string()))?;
            Ok(Value::Bool(re.is_match(&s)))
        }
        Expr::Call { name, args } => {
            if name == "exists" && args.len() == 1 {
                match &args[0] {
                    Expr::Field(fr) => Ok(Value::Bool(ctx.values.contains_key(&fr.as_dotted()))),
                    _ => Err(EvalError::Type("exists expects field ref".into())),
                }
            } else {
                Err(EvalError::Type(format!("unknown function {}", name)))
            }
        }
        Expr::Literal(l) => Ok(literal_to_value(l)),
        Expr::Field(fr) => Ok(ctx.get(fr).ok_or_else(|| EvalError::MissingField(fr.as_dotted()))?.clone()),
    }
}

fn eval_lit_or_field(v: &LiteralOrField, ctx: &EvalContext) -> Result<Value, EvalError> {
    match v {
        LiteralOrField::Lit(l) => Ok(literal_to_value(l)),
        LiteralOrField::Field(fr) => Ok(ctx.get(fr).ok_or_else(|| EvalError::MissingField(fr.as_dotted()))?.clone()),
    }
}

fn literal_to_value(l: &Literal) -> Value {
    match l {
        Literal::Bool(b) => Value::Bool(*b),
        Literal::Number(n) => Value::Number(*n),
        Literal::String(s) => Value::String(s.clone()),
        Literal::Null => Value::Null,
        Literal::Regex(s) => Value::String(s.clone()),
        Literal::List(items) => Value::Set(items.iter().map(|x| match x {
            LiteralOrField::Lit(li) => literal_to_value(li),
            LiteralOrField::Field(fr) => Value::String(fr.as_dotted()),
        }).collect()),
    }
}

fn as_bool(v: &Value) -> Result<bool, EvalError> {
    match v { Value::Bool(b) => Ok(*b), _ => Err(EvalError::Type("expected bool".into())) }
}
fn as_string(v: &Value) -> Result<String, EvalError> {
    match v { Value::String(s) => Ok(s.clone()), _ => Err(EvalError::Type("expected string".into())) }
}

fn compare(op: &CompareOp, a: &Value, b: &Value) -> Result<bool, EvalError> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => Ok(match op {
            CompareOp::Eq => x == y, CompareOp::Ne => x != y,
            CompareOp::Lt => x < y, CompareOp::Le => x <= y,
            CompareOp::Gt => x > y, CompareOp::Ge => x >= y,
        }),
        (Value::String(x), Value::String(y)) => Ok(match op {
            CompareOp::Eq => x == y, CompareOp::Ne => x != y,
            CompareOp::Lt => x < y, CompareOp::Le => x <= y,
            CompareOp::Gt => x > y, CompareOp::Ge => x >= y,
        }),
        (Value::Bool(x), Value::Bool(y)) => Ok(match op {
            CompareOp::Eq => x == y, CompareOp::Ne => x != y,
            _ => return Err(EvalError::Type("ordering not supported for bool".into())),
        }),
        _ => Err(EvalError::Type("incompatible types for compare".into())),
    }
}

fn membership(op: &MembershipOp, item: &Value, target: &Value) -> Result<bool, EvalError> {
    let contained = match target {
        Value::Set(items) => items.iter().any(|v| v == item),
        _ => return Err(EvalError::Type("membership target must be set".into())),
    };
    Ok(match op { MembershipOp::In => contained, MembershipOp::NotIn => !contained })
}

fn contains(container: &Value, needle: &Value) -> Result<bool, EvalError> {
    match (container, needle) {
        (Value::String(s), Value::String(sub)) => Ok(s.contains(sub)),
        (Value::Set(items), v) => Ok(items.iter().any(|x| x == v)),
        _ => Err(EvalError::Type("contains expects string/string or set/T".into())),
    }
}
