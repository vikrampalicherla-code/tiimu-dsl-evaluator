# TIIMU DSL Evaluator (Rust) — v0.1

Includes:
- Parser (`crates/tiimu-dsl`) → AST (`crates/tiimu-expr-ast`)
- Typecheck contracts (`crates/tiimu-expr-typecheck`)
- Evaluator (`crates/tiimu-expr-eval`)
- Discoverability interfaces (`crates/tiimu-expr-registry`)
- Postgres DDL (`db/migrations/001_create_expression_ledger.sql`)

Run:
```bash
cargo test
```
