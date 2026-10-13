//! API key resolution: explicit → keys file → environment.

use std::collections::BTreeMap;
use std::path::Path;

use crate::types::ModelInfo;

/// Resolved credentials. `model_keys` may hold `"provider/model"` overrides.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Keys {
    #[serde(default)]
    pub provider: BTreeMap<String, String>,
    #[serde(default)]
    pub model: BTreeMap<String, String>,
}

impl Keys {
    pub fn load_file(p: &Path) -> Result<Self, crate::types::ApiError> {
        if !p.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(p)
            .map_err(|e| crate::types::ApiError::Other(format!("read {}: {e}", p.display())))?;
        let k: Keys =
            serde_json::from_str(&raw).map_err(crate::types::ApiError::Json)?;
        Ok(k)
    }

    pub fn from_env() -> Self {
        let mut k = Keys::default();
        for (prov, var) in [
            ("openai", "OPENAI_API_KEY"),
            ("openrouter", "OPENROUTER_API_KEY"),
            ("deepseek", "DEEPSEEK_API_KEY"),
            ("anthropic", "ANTHROPIC_API_KEY"),
            ("google", "GOOGLE_API_KEY"),
            ("google", "GEMINI_API_KEY"),
            ("xai", "XAI_API_KEY"),
            ("mistral", "MISTRAL_API_KEY"),
            ("kimi-coding", "MOONSHOT_API_KEY"),
            ("zai", "ZAI_API_KEY"),
            ("moonshotai", "MOONSHOT_API_KEY"),
            ("qwen", "DASHSCOPE_API_KEY"),
            ("groq", "GROQ_API_KEY"),
            ("cerebras", "CEREBRAS_API_KEY"),
            ("together", "TOGETHER_API_KEY"),
            ("fireworks", "FIREWORKS_API_KEY"),
            ("baseten", "BASETEN_API_KEY"),
            ("bedrock", "AWS_BEDROCK_API_KEY"),
            ("github-copilot", "GITHUB_COPILOT_TOKEN"),
            ("hf" /* huggingface */, "HF_TOKEN"),
        ] {
            if let Ok(v) = std::env::var(var) {
                if !v.is_empty() {
                    k.provider.entry(prov.to_string()).or_insert(v);
                }
            }
        }
        k
    }

    /// Best-effort key for a model. Returns None for providers that need no key
    /// (local backends).
    pub fn for_model(&self, m: &ModelInfo) -> Option<String> {
        let key_ref = format!("{}/{}", m.provider, m.id);
        if let Some(k) = self.model.get(&key_ref) {
            return Some(k.clone());
        }
        if m.provider == "ollama" || m.provider == "vllm" || m.provider == "lmstudio" {
            // optional bearer key for secured local servers
            return self.provider.get(&m.provider).cloned();
        }
        self.provider.get(&m.provider).cloned()
    }
}
