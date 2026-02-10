use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExpressionRef {
    Pinned { expression_version_id: String },
    ByLabel { expression_chronicle_id: String, label_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
