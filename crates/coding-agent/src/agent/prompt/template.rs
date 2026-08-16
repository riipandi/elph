//! MiniJinja template engine for the coding-agent domain template.
//!
//! The generic base template lives in `elph-agent` (rendered in Rust); this
//! module renders the product-specific `coding_base.txt` with MiniJinja using
//! custom delimiters (`${{` / `${%`) to avoid collisions with markdown.

use minijinja::{Environment, syntax::SyntaxConfig};

/// Custom delimiters (`${{` / `${%`) to avoid collisions with `{{` in markdown and code examples.
pub fn custom_prompt_syntax() -> SyntaxConfig {
    SyntaxConfig::builder()
        .variable_delimiters("${{", "}}")
        .block_delimiters("${%", "%}")
        .build()
        .expect("valid syntax config")
}

/// Registry for the embedded coding-agent domain template.
#[derive(Clone)]
pub struct PromptTemplateEngine {
    env: Environment<'static>,
}

impl Default for PromptTemplateEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptTemplateEngine {
    pub fn new() -> Self {
        let mut env = Environment::new();
        env.set_syntax(custom_prompt_syntax());
        Self { env }
    }

    pub fn register_embedded(&mut self, name: &'static str, source: &'static str) -> anyhow::Result<()> {
        self.env.add_template(name, source)?;
        Ok(())
    }

    pub fn render<T: serde::Serialize>(&self, name: &str, ctx: &T) -> anyhow::Result<String> {
        let template = self.env.get_template(name)?;
        Ok(template.render(ctx)?)
    }
}

impl std::fmt::Debug for PromptTemplateEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptTemplateEngine").finish_non_exhaustive()
    }
}

/// Shared engine with the coding-agent domain template pre-registered.
pub fn coding_agent_engine() -> PromptTemplateEngine {
    let mut engine = PromptTemplateEngine::new();
    engine
        .register_embedded("coding_base", include_str!("../../../templates/agent/coding_base.txt"))
        .expect("coding_base template is valid");
    engine
}
