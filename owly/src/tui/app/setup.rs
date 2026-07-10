use elph_agent::try_block_on;

use crate::env;
use crate::onboarding::{self, SetupCredentials};

use super::OwlyApp;

impl OwlyApp {
    pub(super) fn complete_setup(&mut self, credentials: SetupCredentials) {
        self.setup.clear_error();
        let apply_context = self.context.clone();
        let persist_context = self.context.clone();

        let config = match try_block_on(async move {
            let snapshot = apply_context.config_snapshot().await;
            onboarding::apply_setup(credentials, &snapshot)
        }) {
            Ok(Ok(config)) => config,
            Ok(Err(err)) => {
                self.setup.set_error(format!("{err:#}"));
                return;
            }
            Err(_) => {
                self.setup.set_error("Failed to apply setup.".to_string());
                return;
            }
        };

        if let Err(err) = env::setup_environment(&config) {
            self.setup.set_error(format!("{err:#}"));
            return;
        }

        if try_block_on(persist_context.replace_config(config.clone())).is_err() {
            self.setup
                .set_error("Failed to update session configuration.".to_string());
            return;
        }

        self.provider = config.provider.clone();
        self.model = config.model_id.clone();
        self.prompt.model_name = config.model_id.clone();
        self.setup_complete = true;
    }
}
