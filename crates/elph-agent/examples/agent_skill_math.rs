//! Example: Math expert skill with real AI model call.
//!
//! This example demonstrates loading a skill and using it with OpenCode big-pickle
//! to answer math questions.
//!
//! ```bash
//! export OPENCODE_API_KEY="your-key"
//! cargo run -p elph-agent --example agent_skill_math
//!
//! # Custom question:
//! cargo run -p elph-agent --example agent_skill_math -- --question "What is 15 * 23?"
//! ```

use std::io::{IsTerminal, Write, stderr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use elph_agent::env::LocalExecutionEnv;
use elph_agent::harness::format_skills_for_system_prompt;
use elph_agent::skills::{format_skill_invocation, load_skills_with_options};
use elph_agent::{Agent, AgentEvent, AgentOptions, PartialAgentState};
use elph_ai::{Message, StopReason, builtin_models, get_builtin_model};
use indicatif::{ProgressBar, ProgressStyle};
use tempfile::TempDir;

const PROVIDER: &str = "opencode";
const MODEL_ID: &str = "big-pickle";

const DEFAULT_QUESTION: &str = "What is 123 + 456?";

struct Args {
    question: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = parse_args()?;

    // ── 1. Create a temporary directory with our math skill ──
    let temp = TempDir::new()?;
    let skill_dir = temp.path().join(".agents").join("skills").join("math-expert");
    std::fs::create_dir_all(&skill_dir)?;

    // ── 2. Write the SKILL.md file ──
    let skill_content = r#"---
name: math-expert
description: Expert mathematician for basic arithmetic. Use when asked to calculate or solve math problems.
license: MIT
compatibility: No special requirements
metadata:
  author: elph-examples
  version: "1.0"
  category: education
allowed-tools: none
---

# Math Expert

You are an expert mathematician specializing in basic arithmetic.

## Capabilities

- Addition (+)
- Subtraction (-)
- Multiplication (*)
- Division (/)
- Order of operations (PEMDAS)

## Instructions

When asked to solve a math problem:

1. **Identify the operation**: Determine what operation(s) are needed
2. **Show your work**: Break down the calculation step by step
3. **Provide the answer**: Give the final result clearly
4. **Verify**: Double-check your calculation

## Example

**Question**: What is 12 * 15?

**Response**:
- Step 1: 12 * 15
- Step 2: 12 * 10 = 120, 12 * 5 = 60
- Step 3: 120 + 60 = 180
- **Answer**: 12 * 15 = 180

## Output Format

Always format your response as:

```
Calculation: [the expression]
Step-by-step: [your work]
Answer: [final result]
```
"#;

    std::fs::write(skill_dir.join("SKILL.md"), skill_content)?;
    println!("Created math-expert skill at: {}\n", skill_dir.display());

    // ── 3. Load the skill ──
    let skill_dir_str = temp.path().join(".agents").join("skills").to_string_lossy().to_string();
    let result = load_skills_with_options(&LocalExecutionEnv::new(temp.path()), &[&skill_dir_str], None).await;

    if result.skills.is_empty() {
        anyhow::bail!("No skills loaded");
    }

    let math_skill = &result.skills[0];
    println!("Loaded skill: {}", math_skill.name);
    println!("Description: {}\n", math_skill.description);

    // ── 4. Show the skill in different formats ──
    println!("=== Skill Invocation Format ===");
    println!("{}\n", format_skill_invocation(math_skill, None));

    println!("=== Skills in System Prompt ===");
    let system_prompt_skills = format_skills_for_system_prompt(&result.skills);
    println!("{}\n", &system_prompt_skills[..system_prompt_skills.len().min(500)]);
    if system_prompt_skills.len() > 500 {
        println!("... (truncated)\n");
    }

    // ── 5. Set up the AI model ──
    if std::env::var("OPENCODE_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .is_none()
    {
        anyhow::bail!(
            "Set OPENCODE_API_KEY to your OpenCode Zen API key.\n\
             Get one at https://opencode.ai"
        );
    }

    let model = get_builtin_model(PROVIDER, MODEL_ID)
        .ok_or_else(|| anyhow::anyhow!("model not found: {PROVIDER}/{MODEL_ID}"))?;

    println!("=== AI Model ===");
    println!("Provider: OpenCode Zen");
    println!("Model:    {} ({})", model.name, model.id);
    println!();

    let setup = progress_spinner("Resolving auth...");
    let models = builtin_models(None);
    let auth = models.get_auth(&model).await?;
    setup.finish_and_clear();

    if let Some(auth) = &auth {
        println!("Auth:     configured via {}", auth.source.as_deref().unwrap_or("unknown"));
    } else {
        anyhow::bail!("OpenCode Zen is not configured (missing OPENCODE_API_KEY?)");
    }
    println!();

    let models: Arc<elph_ai::Models> = models.into_arc();
    let stream_fn: elph_agent::StreamFn = {
        let models = models.clone();
        Arc::new(move |m, ctx, opts| models.stream_simple(m, ctx, opts))
    };

    // ── 6. Build system prompt with skill ──
    let system_prompt = format!(
        r#"You are a helpful math expert.

{skill_invocation}

When answering math questions, follow the instructions in the skill above."#,
        skill_invocation = format_skill_invocation(math_skill, None)
    );

    println!("=== System Prompt (first 500 chars) ===");
    println!("{}...\n", &system_prompt[..system_prompt.len().min(500)]);

    // ── 7. Create the agent ──
    let agent = Agent::new(AgentOptions {
        initial_state: Some(PartialAgentState {
            system_prompt: Some(system_prompt),
            model: Some(model),
            ..Default::default()
        }),
        stream_fn: Some(stream_fn),
        ..Default::default()
    });

    // ── 8. Ask the question ──
    println!("=== Question ===");
    println!("{}\n", args.question);

    println!("=== Answer ===");
    let generating = progress_spinner("Calculating...");
    let saw_delta = Arc::new(AtomicBool::new(false));

    agent
        .subscribe(Arc::new(move |event, _token| {
            let generating = generating.clone();
            let saw_delta = saw_delta.clone();
            Box::pin(async move {
                match event {
                    AgentEvent::MessageUpdate {
                        assistant_message_event,
                        ..
                    } => {
                        if let elph_ai::AssistantMessageEvent::TextDelta { delta, .. } = &*assistant_message_event {
                            if !saw_delta.swap(true, Ordering::SeqCst) {
                                generating.finish_and_clear();
                            }
                            print!("{delta}");
                            let _ = std::io::stdout().flush();
                        }
                    }
                    AgentEvent::AgentEnd { .. } if !saw_delta.load(Ordering::SeqCst) => {
                        generating.finish_and_clear();
                    }
                    AgentEvent::AgentEnd { .. } => {}
                    _ => {}
                }
            })
        }))
        .await;

    agent.prompt_text(&args.question, None).await?;
    agent.wait_for_idle().await;
    println!();

    // ── 9. Show usage stats ──
    let state = agent.state().await;
    println!("\n=== Stats ===");
    println!("Transcript messages: {}", state.messages.len());

    if let Some(Message::Assistant(assistant)) = state.messages.last().and_then(|m| m.as_llm()) {
        println!("Stop reason: {:?}", assistant.stop_reason);
        println!(
            "Tokens: {} in / {} out (total {})",
            assistant.usage.input, assistant.usage.output, assistant.usage.total_tokens
        );
        if let Some(reasoning) = assistant.usage.reasoning {
            println!("Reasoning tokens: {reasoning}");
        }
        if assistant.usage.cost.total > 0.0 {
            println!("Cost: ${:.6}", assistant.usage.cost.total);
        }
        if let Some(error) = &assistant.error_message {
            println!("Error: {error}");
            if assistant.stop_reason == StopReason::Error {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

fn parse_args() -> anyhow::Result<Args> {
    let mut question = DEFAULT_QUESTION.to_string();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--question" | "-q" => {
                question = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--question requires a value"))?;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument: {other}\n\nRun with --help for usage."),
        }
    }

    Ok(Args { question })
}

fn print_help() {
    println!("elph-agent + OpenCode Zen big-pickle Math Skill Example");
    println!();
    println!("Environment:");
    println!("  OPENCODE_API_KEY   Required API key (https://opencode.ai)");
    println!();
    println!("Options:");
    println!("  --question, -q <text>  Math question to answer (default: 'What is 123 + 456?')");
    println!("  -h, --help             Show this help");
    println!();
    println!("Examples:");
    println!("  cargo run -p elph-agent --example agent_skill_math");
    println!("  cargo run -p elph-agent --example agent_skill_math -- --question 'What is 15 * 23?'");
    println!("  cargo run -p elph-agent --example agent_skill_math -q '100 / 4'");
}

fn progress_spinner(message: &str) -> ProgressBar {
    if !progress_enabled() {
        eprintln!("{message}");
        return ProgressBar::hidden();
    }

    let bar = ProgressBar::new_spinner();
    bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg:.cyan}")
            .expect("valid spinner template"),
    );
    bar.set_message(message.to_string());
    bar.enable_steady_tick(Duration::from_millis(80));
    bar
}

fn progress_enabled() -> bool {
    if std::env::var("NO_COLOR").as_deref() == Ok("true") {
        return false;
    }
    stderr().is_terminal()
}
