use std::path::Path;

use crate::metadata::UpdateMetadata;
use crate::prompts::{create_chat_prompt, create_init_prompt, create_update_prompt};

/// Prepare the init command
pub fn prepare_init_command(_cwd: &Path, user_message: Option<&str>, _model: &str) -> (String, String) {
    let system_prompt = create_system_prompt_for_init();
    let user_prompt = create_init_prompt("", user_message);
    (system_prompt, user_prompt)
}

/// Prepare the update command
pub fn prepare_update_command(
    cwd: &Path,
    user_message: Option<&str>,
    _model: &str,
    last_update: Option<&UpdateMetadata>,
) -> (String, String) {
    let system_prompt = create_system_prompt_for_update();
    let git_summary = crate::docs::get_git_summary(cwd);
    let user_prompt = create_update_prompt(last_update, &git_summary, user_message);
    (system_prompt, user_prompt)
}

/// Prepare the chat command
pub fn prepare_chat_command(message: &str) -> (String, String) {
    let system_prompt = create_system_prompt_for_chat();
    let user_prompt = create_chat_prompt(message);
    (system_prompt, user_prompt)
}

fn create_system_prompt_for_init() -> String {
    let base = crate::prompts::create_system_prompt();
    format!(
        "{base}\n\n- This is an initial documentation run.\n- Assume {OWLY_DIR}/ does not yet contain useful documentation.\n- Build the documentation structure from scratch.\n- First build a repository inventory: existing docs, graph/app entrypoints, package/config files, major domain folders, tests/evals, data/schema files, skill/playbook files, and operational scripts.\n- Use git evidence during init to understand how important files and workflows came to be.\n- Create {OWLY_DIR}/quickstart.md first, then the linked section pages.\n- Use at most 8 documentation pages on the initial run unless the repository is clearly tiny.\n- Do not try to document every source file. Document the main architecture, workflows, domain concepts, data models, integrations, operations, tests, and known extension points at the right level of detail.\n- The CLI will record successful run metadata only when documentation content changes.",
        OWLY_DIR = crate::constants::OWLY_DIR
    )
}

fn create_system_prompt_for_update() -> String {
    let base = crate::prompts::create_system_prompt();
    format!(
        "{base}\n\n- This is a maintenance update run.\n- Inspect the existing {OWLY_DIR}/ documentation before editing.\n- Always use git-oriented repository evidence to understand recent changes.\n- Before editing, build a docs impact plan from the changed source files.\n- Update runs must be surgical. Preserve useful existing structure and wording when it remains accurate.\n- Only edit pages whose current content is inaccurate, incomplete, or misleading because of the recent changes.\n- Keep each concept in one canonical page.\n- Do not make formatting-only edits.\n- Use a soft diff budget: if fewer than about 5 source files changed, update at most 1-2 wiki pages.\n- Updates may be a no-op. If there are no relevant changes, do not edit files.\n- The CLI will record successful run metadata only when documentation content changes.",
        OWLY_DIR = crate::constants::OWLY_DIR
    )
}

fn create_system_prompt_for_chat() -> String {
    let base = crate::prompts::create_system_prompt();
    format!(
        "{base}\n\n- This is an interactive chat turn.\n- Answer the user's message directly.\n- Do not create or update Owly documentation unless the user explicitly asks you to modify documentation.\n- If the user asks to initialize or update the wiki, explain that they can run owly --init or owly --update."
    )
}
