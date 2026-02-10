use regex::Regex;
use thiserror::Error;
use tiimu_expr_ast::{CompareOp, Expr, FieldRef, Literal, LiteralOrField};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty { Bool, Number, String, Null, Set(Box<Ty>), Any }

pub trait Dictionary {
    fn field_type(&self, field: &FieldRef) -> Option<Ty>;
}

pub trait FunctionRegistry {
    fn function_signature(&self, name: &str) -> Option<(Vec<Ty>, Ty)>;
}

#[derive(Debug, Error)]
pub enum TypeError {
    #[error("unknown field: {0}")]
    UnknownField(String),
    #[error("type mismatch: {0}")]
    TypeMismatch(String),
    #[error("unknown function: {0}")]
    UnknownFunction(String),
    #[error("invalid regex: {0}")]
    InvalidRegex(String),
    #[error("expression must evaluate to boolean")]
    NotBoolean,
}

pub fn typecheck(expr: &Expr, dict: &dyn Dictionary, fns: &dyn FunctionRegistry) -> Result<Ty, TypeError> {
    let ty = infer(expr, dict, fns)?;
    if ty != Ty::Bool { return Err(TypeError::NotBoolean); }
    Ok(ty)
}

fn infer(expr: &Expr, dict: &dyn Dictionary, fns: &dyn FunctionRegistry) -> Result<Ty, TypeError> {
    use tiimu_expr_ast::{LogicalOp, MembershipOp};
    match expr {
        Expr::Not(e) => { ensure_bool(infer(e, dict, fns)?, "! expects bool")?; Ok(Ty::Bool) }
        Expr::Logical{lhs, rhs, ..} => {
            ensure_bool(infer(lhs, dict, fns)?, "lhs must be bool")?;
            ensure_bool(infer(rhs, dict, fns)?, "rhs must be bool")?;
            Ok(Ty::Bool)
        }
        Expr::Compare{field, op, value} => {
            let ft = dict.field_type(field).ok_or_else(|| TypeError::UnknownField(field.as_dotted()))?;
            let vt = infer_value(value, dict)?;
            match (&ft, &vt) {
                (Ty::Number, Ty::Number) | (Ty::String, Ty::String) | (Ty::Bool, Ty::Bool) => Ok(Ty::Bool),
                (t, Ty::Null) | (Ty::Null, t) => match op {
                    CompareOp::Eq | CompareOp::Ne => Ok(Ty::Bool),
                    _ => Err(TypeError::TypeMismatch("null only with == or !=".into())),
                },
                _ => Err(TypeError::TypeMismatch(format!("cannot compare {:?} with {:?}", ft, vt))),
            }
        }
        Expr::Membership{field, op: _op, list} => {
            let ft = dict.field_type(field).ok_or_else(|| TypeError::UnknownField(field.as_dotted()))?;
            match (&ft, list) {
                (Ty::String, LiteralOrField::Lit(Literal::List(_))) => Ok(Ty::Bool),
                (Ty::Number, LiteralOrField::Lit(Literal::List(_))) => Ok(Ty::Bool),
                (Ty::Set(_), LiteralOrField::Lit(Literal::List(_))) => Ok(Ty::Bool),
                (Ty::String, LiteralOrField::Field(fr)) => {
                    match dict.field_type(fr).ok_or_else(|| TypeError::UnknownField(fr.as_dotted()))? {
                        Ty::Set(inner) if *inner == Ty::String => Ok(Ty::Bool),
                        _ => Err(TypeError::TypeMismatch("membership expects set<string>".into())),
                    }
                }
                _ => Err(TypeError::TypeMismatch("invalid membership usage".into())),
            }
        }
        Expr::Contains{field, value, ..} => {
            let ft = dict.field_type(field).ok_or_else(|| TypeError::UnknownField(field.as_dotted()))?;
            let vt = infer_value(value, dict)?;
            match ft {
                Ty::String if vt == Ty::String => Ok(Ty::Bool),
                Ty::Set(inner) if *inner == vt => Ok(Ty::Bool),
                _ => Err(TypeError::TypeMismatch("contains requires string/string or set<T>/T".into())),
            }
        }
        Expr::RegexMatch{field, pattern} => {
            let ft = dict.field_type(field).ok_or_else(|| TypeError::UnknownField(field.as_dotted()))?;
            if ft != Ty::String { return Err(TypeError::TypeMismatch("regex needs string field".into())); }
            Regex::new(pattern).map_err(|e| TypeError::InvalidRegex(e.to_string()))?;
            Ok(Ty::Bool)
        }
        Expr::Call{name, args} => {
            let (params, ret) = fns.function_signature(name).ok_or_else(|| TypeError::UnknownFunction(name.clone()))?;
            if params.len() != args.len() { return Err(TypeError::TypeMismatch("arg count mismatch".into())); }
            for (a, p) in args.iter().zip(params.iter()) {
                let at = infer(a, dict, fns)?;
                if *p != Ty::Any && at != *p { return Err(TypeError::TypeMismatch("arg type mismatch".into())); }
            }
            Ok(ret)
        }
        Expr::Literal(l) => Ok(match l {
            Literal::Bool(_) => Ty::Bool,
            Literal::Number(_) => Ty::Number,
            Literal::String(_) => Ty::String,
            Literal::Null => Ty::Null,
            Literal::Regex(_) => Ty::String,
            Literal::List(_) => Ty::Any,
        }),
        Expr::Field(fr) => dict.field_type(fr).ok_or_else(|| TypeError::UnknownField(fr.as_dotted())),
    }
}

fn infer_value(v: &LiteralOrField, dict: &dyn Dictionary) -> Result<Ty, TypeError> {
    match v {
        LiteralOrField::Lit(l) => Ok(match l {
            Literal::Bool(_) => Ty::Bool,
            Literal::Number(_) => Ty::Number,
            Literal::String(_) => Ty::String,
            Literal::Null => Ty::Null,
            Literal::Regex(_) => Ty::String,
            Literal::List(_) => Ty::Any,
        }),
        LiteralOrField::Field(fr) => dict.field_type(fr).ok_or_else(|| TypeError::UnknownField(fr.as_dotted())),
    }
}

fn ensure_bool(t: Ty, msg: &str) -> Result<(), TypeError> {
    if t != Ty::Bool { Err(TypeError::TypeMismatch(msg.into())) } else { Ok(()) }
}
