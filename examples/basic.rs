use std::collections::HashMap;
use tiimu_dsl::parse_expression;
use tiimu_expr_eval::{eval, EvalContext, Value};

fn main() {
    let expr = parse_expression(r#"customer.is_known == true && signal.page_views_7d >= 2"#).unwrap();
    let mut map = HashMap::new();
    map.insert("customer.is_known".into(), Value::Bool(true));
    map.insert("signal.page_views_7d".into(), Value::Number(3.0));
    let ctx = EvalContext::new(map);
    println!("{}", eval(&expr, &ctx).unwrap());
}
