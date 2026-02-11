use std::collections::HashMap;
use std::sync::Arc;

use tiimu_dsl::parse_expression;
use tiimu_expr_ast::extract_dependencies;
use tiimu_expr_eval::{eval_with_registry, EvalContext, Value, Function, FunctionRegistry, FunctionSignature, ValueTy};

/// Example custom function: starts_with(str, prefix) -> bool
struct StartsWithFn;
impl Function for StartsWithFn {
    fn name(&self) -> &'static str { "starts_with" }
    fn signature(&self) -> FunctionSignature {
        FunctionSignature { params: vec![ValueTy::String, ValueTy::String], ret: ValueTy::Bool }
    }
    fn call(&self, args: &[Value], _ctx: &EvalContext) -> Result<Value, tiimu_expr_eval::EvalError> {
        if args.len() != 2 { return Err(tiimu_expr_eval::EvalError::Type("starts_with expects 2 args".into())); }
        match (&args[0], &args[1]) {
            (Value::String(s), Value::String(p)) => Ok(Value::Bool(s.starts_with(p))),
            _ => Err(tiimu_expr_eval::EvalError::Type("starts_with expects (string,string)".into())),
        }
    }
}

fn main() {
    let expr = parse_expression(r#"starts_with(signal.entry_source, "so") && customer.is_known == true"#).unwrap();

    let deps = extract_dependencies(&expr);
    println!("fields={:?}", deps.fields);
    println!("functions={:?}", deps.functions);

    let mut values = HashMap::new();
    values.insert("signal.entry_source".into(), Value::String("social".into()));
    values.insert("customer.is_known".into(), Value::Bool(true));
    let ctx = EvalContext::new(values);

    let mut reg = FunctionRegistry::with_builtins();
    reg.register(Arc::new(StartsWithFn));

    let result = eval_with_registry(&expr, &ctx, &reg).unwrap();
    println!("result={}", result);
}
