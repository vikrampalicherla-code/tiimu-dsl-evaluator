//!
//! Runtime evaluator for TIIMU expressions.
//!
//! Responsibilities:
//! - Evaluate AST (`tiimu-expr-ast::Expr`) against `EvalContext`.
//! - Short-circuit semantics for `&&` / `||`.
//! - Pluggable functions via `FunctionRegistry`.
//!
//! Assumptions:
//! - Expressions are deploy-time validated, so runtime should not see unknown fields/functions.
//! - Expressions are stateless unless composed by traversal logic.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tiimu_expr_ast::{CompareOp, Expr, FieldRef, Literal, LiteralOrField, LogicalOp, MembershipOp};


use std::sync::Arc;

/// Runtime value type for optional signature checks in the evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueTy {
    Bool,
    Number,
    String,
    Null,
    Set,
    Any,
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub params: Vec<ValueTy>,
    pub ret: ValueTy,
}

/// Pluggable function implementation.
/// Pluggable function implementation for `Expr::Call`.
///
/// Functions should be deterministic and side-effect free.
/// The evaluator may short-circuit, so functions are only called if needed.
pub trait Function: Send + Sync {
    fn name(&self) -> &'static str;
    fn signature(&self) -> FunctionSignature;
    fn call(&self, args: &[Value], ctx: &EvalContext) -> Result<Value, EvalError>;
}

/// Registry for functions used by `Expr::Call`.
#[derive(Default, Clone)]
/// Registry for callable functions referenced from expressions.
///
/// Supports "write once, reuse many" by allowing shared functions.
pub struct FunctionRegistry {
    funcs: HashMap<String, Arc<dyn Function>>,
}

impl FunctionRegistry {
    pub fn new() -> Self { Self { funcs: HashMap::new() } }

    /// Default registry that includes TIIMU builtins.
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.register(Arc::new(LenFn));
        r
    }

    pub fn register(&mut self, f: Arc<dyn Function>) {
        self.funcs.insert(f.name().to_string(), f);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Function>> {
        self.funcs.get(name).cloned()
    }
}

/// Builtin: len(x) -> number
/// - len(string) = string length
/// - len(set) = set length
pub struct LenFn;

impl Function for LenFn {
    fn name(&self) -> &'static str { "len" }

    fn signature(&self) -> FunctionSignature {
        FunctionSignature { params: vec![ValueTy::Any], ret: ValueTy::Number }
    }

    fn call(&self, args: &[Value], _ctx: &EvalContext) -> Result<Value, EvalError> {
        if args.len() != 1 { return Err(EvalError::Type("len expects 1 arg".into())); }
        match &args[0] {
            Value::String(s) => Ok(Value::Number(s.chars().count() as f64)),
            Value::Set(v) => Ok(Value::Number(v.len() as f64)),
            _ => Err(EvalError::Type("len expects string or set".into())),
        }
    }
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Runtime value used by the evaluator.
///
/// Keep this intentionally small for edge execution.
pub enum Value {
    Bool(bool),
    Number(f64),
    String(String),
    Null,
    Set(Vec<Value>),
}

#[derive(Debug, Clone)]
/// Runtime context for evaluation.
///
/// Keys are dotted field refs (e.g. `signal.page_views_7d`).
/// Values are typed `Value`s.
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

/// Evaluate using the default builtin function registry.
///
/// For custom functions, use `eval_with_registry`.
pub fn eval(expr: &Expr, ctx: &EvalContext) -> Result<bool, EvalError> {
    eval_with_registry(expr, ctx, &FunctionRegistry::with_builtins())
}

/// Evaluate with a caller-provided function registry.
/// Evaluate using a caller-provided function registry.
///
/// Recommended entrypoint for tenant-specific functions.
pub fn eval_with_registry(expr: &Expr, ctx: &EvalContext, fns: &FunctionRegistry) -> Result<bool, EvalError> {

    match eval_value(expr, ctx)? {
        Value::Bool(b) => Ok(b),
        _ => Err(EvalError::Type("top-level must be bool".into())),
    }
}

fn eval_value(expr: &Expr, ctx: &EvalContext, fns: &FunctionRegistry) -> Result<Value, EvalError> {
    match expr {
        Expr::Not(e) => Ok(Value::Bool(!as_bool(&eval_value(e, ctx, fns)?)?)),
        Expr::Logical { op, lhs, rhs } => match op {
            LogicalOp::And => {
                let l = as_bool(&eval_value(lhs, ctx, fns)?)?;
                if !l { return Ok(Value::Bool(false)); }
                let r = as_bool(&eval_value(rhs, ctx, fns)?)?;
                Ok(Value::Bool(l && r))
            }
            LogicalOp::Or => {
                let l = as_bool(&eval_value(lhs, ctx, fns)?)?;
                if l { return Ok(Value::Bool(true)); }
                let r = as_bool(&eval_value(rhs, ctx, fns)?)?;
                Ok(Value::Bool(l || r))
            }
        },
        Expr::Compare { field, op, value } => {
            let fv = ctx.get(field).ok_or_else(|| EvalError::MissingField(field.as_dotted()))?.clone();
            let vv = eval_lit_or_field(value, ctx, fns)?;
            Ok(Value::Bool(compare(op, &fv, &vv)?))
        }
        Expr::Membership { field, op, list } => {
            let fv = ctx.get(field).ok_or_else(|| EvalError::MissingField(field.as_dotted()))?.clone();
            let target = eval_lit_or_field(list, ctx, fns)?;
            Ok(Value::Bool(membership(op, &fv, &target)?))
        }
        Expr::Contains { field, value, .. } => {
            let fv = ctx.get(field).ok_or_else(|| EvalError::MissingField(field.as_dotted()))?.clone();
            let vv = eval_lit_or_field(value, ctx, fns)?;
            Ok(Value::Bool(contains(&fv, &vv)?))
        }
        Expr::RegexMatch { field, pattern } => {
            let fv = ctx.get(field).ok_or_else(|| EvalError::MissingField(field.as_dotted()))?.clone();
            let s = as_string(&fv)?;
            let re = Regex::new(pattern).map_err(|e| EvalError::Regex(e.to_string()))?;
            Ok(Value::Bool(re.is_match(&s)))
        }
        
Expr::Call { name, args } => {
    // Special-form: exists(field_ref) -> bool
    if name == "exists" && args.len() == 1 {
        if let Expr::Field(fr) = &args[0] {
            return Ok(Value::Bool(ctx.values.contains_key(&fr.as_dotted())));
        }
    }

    // Evaluate args (pure expressions)
    let mut argv = Vec::with_capacity(args.len());
    for a in args {
        argv.push(eval_value(a, ctx, fns)?);
    }

    let f = fns.get(name).ok_or_else(|| EvalError::Type(format!("unknown function {}", name)))?;
    f.call(&argv, ctx)
}
Expr::Literal(l) => Ok(literal_to_value(l)),
        Expr::Field(fr) => Ok(ctx.get(fr).ok_or_else(|| EvalError::MissingField(fr.as_dotted()))?.clone()),
    }
}

fn eval_lit_or_field(v: &LiteralOrField, ctx: &EvalContext, _fns: &FunctionRegistry) -> Result<Value, EvalError> {
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
