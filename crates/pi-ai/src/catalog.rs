//! Model catalog: built-in models + local overlay (`$PI_HOME/models.json`).
//!
//! The overlay file has the same shape as the built-in list, enabling users to
//! add any custom model/endpoint without touching code (extra over pi's static
//! catalog, which overlays a network-fetched revision).

use std::sync::OnceLock;

use crate::types::ModelInfo;

const BUILTIN: &str = include_str!("models.json");

/// All known models.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub models: Vec<ModelInfo>,
}

impl Catalog {
    pub fn builtin() -> &'static Self {
        static CAT: OnceLock<Catalog> = OnceLock::new();
        CAT.get_or_init(|| {
            let models: Vec<ModelInfo> = serde_json::from_str(BUILTIN)
                .expect("embedded models.json must parse");
            Catalog { models }
        })
    }

    /// Built-in catalog merged with overlay files (later files win for same
    /// provider+id; new entries append).
    pub fn with_overlays(paths: &[std::path::PathBuf]) -> Result<Self, crate::types::ApiError> {
        let mut cat = Self::builtin().clone();
        for p in paths {
            if !p.exists() {
                continue;
            }
            let raw = std::fs::read_to_string(p).map_err(|e| {
                crate::types::ApiError::Other(format!("read {}: {e}", p.display()))
            })?;
            let extra: Vec<ModelInfo> =
                serde_json::from_str(&raw).map_err(|e| {
                    crate::types::ApiError::Other(format!("{}: {e}", p.display()))
                })?;
            for m in extra {
                match cat
                    .models
                    .iter_mut()
                    .find(|c| c.id == m.id && c.provider == m.provider)
                {
                    Some(slot) => *slot = m,
                    None => cat.models.push(m),
                }
            }
        }
        Ok(cat)
    }

    pub fn get(&self, provider: &str, id: &str) -> Option<&ModelInfo> {
        self.models.iter().find(|m| m.provider == provider && m.id == id)
    }

    /// Resolve `provider/model-id` (or a bare model id that matches exactly one).
    pub fn resolve(&self, refstr: &str) -> Option<ModelInfo> {
        if let Some((prov, id)) = refstr.split_once('/') {
            if let Some(m) = self.get(prov, id) {
                return Some(m.clone());
            }
        }
        let hits: Vec<&ModelInfo> = self
            .models
            .iter()
            .filter(|m| m.id == refstr)
            .collect();
        if hits.len() == 1 {
            Some(hits[0].clone())
        } else {
            None
        }
    }

    pub fn by_provider(&self, provider: &str) -> Vec<ModelInfo> {
        self.models
            .iter()
            .filter(|m| m.provider == provider)
            .cloned()
            .collect()
    }

    pub fn all(&self) -> &[ModelInfo] {
        &self.models
    }

    pub fn cost_usd(&self, m: &ModelInfo, u: &crate::types::Usage) -> f64 {
        let inp = u.input as f64 + u.cache_read as f64 + u.cache_write as f64;
        inp / 1e6 * m.input_price + u.output as f64 / 1e6 * m.output_price
    }
}
