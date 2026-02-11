//!
//! AST types for TIIMU's Text DSL.
//!
//! This crate is intentionally small and shared by:
//! - the DSL parser (compile-time),
//! - the typechecker (deploy-time validation),
//! - the evaluator (runtime),
//! - and storage/indexing tooling (dependency extraction, hashing).
//!
//! Key features:
//! - `Expr`: the expression AST used across the system.
//! - `ast_hash`: stable hash of the canonical JSON representation (dedupe / caching).
//! - `extract_dependencies`: walks the AST and returns referenced fields and functions.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
/// A dotted path reference like `customer.is_known`.
///
/// Stored as a vector of identifiers to avoid repeated splitting.
pub struct FieldRef {
    pub path: Vec<String>,
}
impl FieldRef {
    pub fn new(path: Vec<String>) -> Self { Self { path } }
    pub fn as_dotted(&self) -> String { self.path.join(".") }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    Bool(bool),
    Number(f64),
    String(String),
    Null,
    Regex(String),
    List(Vec<LiteralOrField>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LiteralOrField {
    Lit(Literal),
    Field(FieldRef),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompareOp { Eq, Ne, Lt, Le, Gt, Ge }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MembershipOp { In, NotIn }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContainsOp { Contains }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LogicalOp { And, Or }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Expression AST.
///
/// Notes:
/// - Logical ops use short-circuit at runtime.
/// - `Call` is for extensible functions resolved via a registry.
pub enum Expr {
    Not(Box<Expr>),
    Logical { op: LogicalOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Compare { field: FieldRef, op: CompareOp, value: LiteralOrField },
    Membership { field: FieldRef, op: MembershipOp, list: LiteralOrField },
    Contains { field: FieldRef, op: ContainsOp, value: LiteralOrField },
    RegexMatch { field: FieldRef, pattern: String },
    Call { name: String, args: Vec<Expr> },
    Literal(Literal),
    Field(FieldRef),
}

pub fn canonical_json(expr: &Expr) -> serde_json::Value {
    serde_json::to_value(expr).expect("Expr serializable")
}

pub fn ast_hash(expr: &Expr) -> String {
    let v = canonical_json(expr);
    let bytes = serde_json::to_vec(&v).expect("json");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}


use std::collections::HashSet;

/// Dependencies extracted from an expression: field references and function calls.
///
/// This powers:
/// - deploy-time validation (detect unknown fields/functions),
/// - storage indexing (`expression_dependencies`),
/// - impact analysis (“what breaks if field X changes?”).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Dependencies {
    pub fields: HashSet<String>,
    pub functions: HashSet<String>,
}

/// Walks the AST and returns the set of dotted field references and function names.
pub fn extract_dependencies(expr: &Expr) -> Dependencies {
    let mut d = Dependencies::default();
    walk(expr, &mut d);
    d
}

fn add_field(d: &mut Dependencies, fr: &FieldRef) {
    d.fields.insert(fr.as_dotted());
}

fn add_lit_or_field(d: &mut Dependencies, v: &LiteralOrField) {
    match v {
        LiteralOrField::Field(fr) => add_field(d, fr),
        LiteralOrField::Lit(Literal::List(items)) => {
            for it in items {
                add_lit_or_field(d, it);
            }
        }
        _ => {}
    }
}

fn walk(expr: &Expr, d: &mut Dependencies) {
    match expr {
        Expr::Not(e) => walk(e, d),
        Expr::Logical { lhs, rhs, .. } => { walk(lhs, d); walk(rhs, d); }
        Expr::Compare { field, value, .. } => { add_field(d, field); add_lit_or_field(d, value); }
        Expr::Membership { field, list, .. } => { add_field(d, field); add_lit_or_field(d, list); }
        Expr::Contains { field, value, .. } => { add_field(d, field); add_lit_or_field(d, value); }
        Expr::RegexMatch { field, .. } => { add_field(d, field); }
        Expr::Call { name, args } => {
            d.functions.insert(name.clone());
            for a in args { walk(a, d); }
        }
        Expr::Literal(_) => {}
        Expr::Field(fr) => add_field(d, fr),
    }
}
