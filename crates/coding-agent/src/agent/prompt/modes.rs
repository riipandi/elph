//! Agent mode guidance - mode footer slug for template rendering.

use crate::types::AgentMode;

pub fn mode_footer_slug(mode: AgentMode) -> &'static str {
    mode.footer_label()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_has_footer_slug() {
        for mode in [AgentMode::Build, AgentMode::Plan, AgentMode::Ask, AgentMode::Brave] {
            assert!(!mode_footer_slug(mode).is_empty());
        }
    }
}
