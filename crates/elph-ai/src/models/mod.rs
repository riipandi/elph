pub mod catalog;

mod collection;

pub use catalog::{get_builtin_model, get_builtin_models, get_builtin_providers};
pub use collection::*;

use crate::types::{Model, ThinkingLevel, Usage};

pub fn calculate_cost(model: &Model, usage: &mut Usage) {
    let long_write = usage.cache_write_1h.unwrap_or(0);
    let short_write = usage.cache_write.saturating_sub(long_write);
    let m = 1_000_000.0;
    usage.cost.input = (model.cost.input / m) * usage.input as f64;
    usage.cost.output = (model.cost.output / m) * usage.output as f64;
    usage.cost.cache_read = (model.cost.cache_read / m) * usage.cache_read as f64;
    usage.cost.cache_write =
        (model.cost.cache_write * short_write as f64 + model.cost.input * 2.0 * long_write as f64) / m;
    usage.cost.total = usage.cost.input + usage.cost.output + usage.cost.cache_read + usage.cost.cache_write;
}

pub fn thinking_level_to_str(level: ThinkingLevel) -> &'static str {
    match level {
        ThinkingLevel::Minimal => "minimal",
        ThinkingLevel::Low => "low",
        ThinkingLevel::Medium => "medium",
        ThinkingLevel::High => "high",
        ThinkingLevel::Xhigh => "xhigh",
    }
}
