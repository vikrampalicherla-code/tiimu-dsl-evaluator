use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
