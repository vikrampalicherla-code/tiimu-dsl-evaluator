//!
//! Storage/discoverability interfaces for expressions.
//!
//! Defines:
//! - how other artifacts reference expressions (`ExpressionRef`),
//! - how we record where-used (`ExpressionUsage`),
//! - the minimal registry trait.
//!
//! Concrete storage lives in TIIMU service crates (e.g., Postgres-backed).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
/// Reference to an expression from another artifact.
///
/// - `Pinned`: immutable reference to a specific expression version.
/// - `ByLabel`: resolves to a version at publish/compile time (e.g., `current`).
pub enum ExpressionRef {
    Pinned { expression_version_id: String },
    ByLabel { expression_chronicle_id: String, label_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Record of an expression being used by another artifact.
///
/// Supports discoverability (“where is this used?”) and change impact analysis.
pub struct ExpressionUsage {
    pub expression_ref: ExpressionRef,
    pub referencer_type: String,
    pub referencer_id: String,
    pub referencer_version_id: String,
    pub role: String,
    pub path: Option<String>,
}

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("not found")]
    NotFound,
    #[error("storage error: {0}")]
    Storage(String),
}

#[async_trait]
pub trait ExpressionRegistry: Send + Sync {
    async fn resolve_label(&self, chronicle_id: &str, label: &str) -> Result<String, RegistryError>;
    async fn record_usage(&self, usage: ExpressionUsage) -> Result<(), RegistryError>;
    async fn list_usages(&self, expression_version_id_or_chronicle: &str) -> Result<Vec<ExpressionUsage>, RegistryError>;
}
