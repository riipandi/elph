//! Agent prompt execution and run lifecycle.

use std::collections::HashSet;
use std::sync::Arc;

use elph_ai::{ImageContent, Message, SimpleStreamOptions, UserContent};
use tokio::sync::{Mutex, oneshot};
use tokio_util::sync::CancellationToken;

use crate::agent_loop::{run_agent_loop, run_agent_loop_continue};
use crate::types::{AgentContext, AgentLoopConfig, AgentMessage, AgentThinkingLevel};

use super::events::{now_ms, process_event};
use super::state::default_model;
use super::{ActiveRun, Agent};

impl Agent {
    pub async fn prompt_text(&self, text: impl Into<String>, images: Option<Vec<ImageContent>>) -> anyhow::Result<()> {
        let mut content: Vec<elph_ai::ContentBlock> = vec![elph_ai::ContentBlock::Text { text: text.into() }];
        if let Some(images) = images {
            for image in images {
                content.push(elph_ai::ContentBlock::Image {
                    data: image.data,
                    mime_type: image.mime_type,
                });
            }
        }
        self.prompt_messages(vec![crate::types::llm_message_to_agent(Message::User {
            content: UserContent::Blocks(content),
            timestamp: now_ms(),
        })])
        .await
    }

    pub async fn prompt_messages(&self, messages: Vec<AgentMessage>) -> anyhow::Result<()> {
        if self.active_run.lock().await.is_some() {
            anyhow::bail!(
                "Agent is already processing a prompt. Use steer() or followUp() to queue messages, or wait for completion."
            );
        }
        self.run_prompt_messages(messages, false).await
    }

    pub async fn continue_run(&self) -> anyhow::Result<()> {
        if self.active_run.lock().await.is_some() {
            anyhow::bail!("Agent is already processing. Wait for completion before continuing.");
        }

        let last = self.state.lock().await.messages().last().cloned();
        let Some(last) = last else {
            anyhow::bail!("No messages to continue from");
        };

        if last.role() == "assistant" {
            let steering = self.steering_queue.drain();
            if !steering.is_empty() {
                return self.run_prompt_messages(steering, true).await;
            }
            let follow_up = self.follow_up_queue.drain();
            if !follow_up.is_empty() {
                return self.run_prompt_messages(follow_up, false).await;
            }
            anyhow::bail!("Cannot continue from message role: assistant");
        }

        self.run_continuation().await
    }

    async fn run_prompt_messages(
        &self,
        messages: Vec<AgentMessage>,
        skip_initial_steering: bool,
    ) -> anyhow::Result<()> {
        self.skip_initial_steering
            .store(skip_initial_steering, std::sync::atomic::Ordering::SeqCst);
        let context = self.create_context_snapshot().await;
        let config = self.create_loop_config();
        let token = self.begin_run().await?;
        let emit = self.create_emit_callback(token.clone());

        run_agent_loop(messages, context, config, emit, Some(token))
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
        self.finish_run().await;
        Ok(())
    }

    async fn run_continuation(&self) -> anyhow::Result<()> {
        self.skip_initial_steering
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let context = self.create_context_snapshot().await;
        let config = self.create_loop_config();
        let token = self.begin_run().await?;
        let emit = self.create_emit_callback(token.clone());

        run_agent_loop_continue(context, config, emit, Some(token))
            .await
            .map_err(|error| anyhow::anyhow!(error))?;
        self.finish_run().await;
        Ok(())
    }

    async fn create_context_snapshot(&self) -> AgentContext {
        let state = self.state.lock().await;
        AgentContext {
            system_prompt: state.system_prompt().to_string(),
            messages: state.messages().to_vec(),
            tools: state.tools().to_vec(),
        }
    }

    fn create_loop_config(&self) -> AgentLoopConfig {
        let steering_queue = self.steering_queue.clone();
        let follow_up_queue = self.follow_up_queue.clone();
        let skip_flag = self.skip_initial_steering.clone();

        let get_steering: crate::types::GetQueuedMessagesFn = Arc::new({
            let steering_queue = steering_queue.clone();
            let skip_flag = skip_flag.clone();
            move || {
                let steering_queue = steering_queue.clone();
                let skip_flag = skip_flag.clone();
                Box::pin(async move {
                    if skip_flag.swap(false, std::sync::atomic::Ordering::SeqCst) {
                        return Vec::new();
                    }
                    steering_queue.drain()
                })
            }
        });

        let get_follow_up: crate::types::GetQueuedMessagesFn = Arc::new({
            let follow_up_queue = follow_up_queue.clone();
            move || {
                let follow_up_queue = follow_up_queue.clone();
                Box::pin(async move { follow_up_queue.drain() })
            }
        });

        let state = self.state.try_lock().ok();
        let (model, thinking_level) = state
            .as_ref()
            .map(|s| (s.model().clone(), s.thinking_level()))
            .unwrap_or_else(|| (default_model(), AgentThinkingLevel::Off));

        let mut stream_options = SimpleStreamOptions {
            base: Default::default(),
            reasoning: thinking_level.to_stream_reasoning(),
            thinking_budgets: self.thinking_budgets.clone(),
        };
        stream_options.base.session_id = self.session_id.lock().clone();
        stream_options.base.transport = Some(self.transport);
        stream_options.base.max_retry_delay_ms = self.max_retry_delay_ms;
        stream_options.base.on_payload = self.on_payload.clone();
        stream_options.base.on_response = self.on_response.clone();

        AgentLoopConfig {
            model,
            stream_options,
            convert_to_llm: self.convert_to_llm.clone(),
            transform_context: self.transform_context.clone(),
            get_api_key: self.get_api_key.clone(),
            should_stop_after_turn: None,
            prepare_next_turn: {
                let with_context = self.prepare_next_turn.clone();
                let legacy = self.prepare_next_turn_legacy.clone();
                let token_holder = self.current_abort_token.clone();
                if with_context.is_none() && legacy.is_none() {
                    None
                } else {
                    Some(Arc::new(move |ctx| {
                        let with_context = with_context.clone();
                        let legacy = legacy.clone();
                        let token_holder = token_holder.clone();
                        Box::pin(async move {
                            if let Some(callback) = with_context {
                                callback(ctx).await
                            } else if let Some(callback) = legacy {
                                let signal = token_holder.lock().await.clone();
                                callback(signal).await
                            } else {
                                None
                            }
                        })
                    }))
                }
            },
            get_steering_messages: Some(get_steering),
            get_follow_up_messages: Some(get_follow_up),
            tool_execution: self.tool_execution,
            before_tool_call: self.before_tool_call.clone(),
            after_tool_call: self.after_tool_call.clone(),
            stream_fn: Some(self.stream_fn.clone()),
            prompt_encoding: self.prompt_encoding.clone(),
        }
    }

    fn create_emit_callback(&self, token: CancellationToken) -> crate::agent_loop::AgentEventCallback {
        let state = self.state.clone();
        let listeners = self.listeners.clone();

        Arc::new(move |event| {
            let state = state.clone();
            let listeners = listeners.clone();
            let token = token.clone();
            Box::pin(async move {
                process_event(&state, &event, &listeners, &token).await;
            })
        })
    }

    async fn begin_run(&self) -> anyhow::Result<CancellationToken> {
        if self.active_run.lock().await.is_some() {
            anyhow::bail!("Agent is already processing.");
        }
        let (idle_tx, idle_rx) = oneshot::channel();
        let abort_token = CancellationToken::new();
        {
            let mut state = self.state.lock().await;
            state.set_streaming(true);
            state.clear_error();
        }
        *self.current_abort_token.lock().await = Some(abort_token.clone());
        *self.active_run.lock().await = Some(ActiveRun {
            idle_tx,
            idle_rx: Mutex::new(Some(idle_rx)),
            abort_token: abort_token.clone(),
        });
        Ok(abort_token)
    }

    async fn finish_run(&self) {
        {
            let mut state = self.state.lock().await;
            state.set_streaming(false);
            state.set_streaming_message(None);
            state.set_pending_tool_calls(HashSet::new());
        }
        *self.current_abort_token.lock().await = None;
        if let Some(run) = self.active_run.lock().await.take() {
            let _ = run.idle_tx.send(());
        }
    }
}
