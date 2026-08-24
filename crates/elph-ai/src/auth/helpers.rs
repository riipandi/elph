use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use super::types::{
    ApiKeyAuth, ApiKeyCredential, AuthContext, AuthLoginCallbacks, AuthModel, AuthResolveInput, AuthResult,
};
use super::types::{ModelAuth, OAuthAuth};

pub fn env_api_key_auth(name: impl Into<String>, env_vars: Vec<&'static str>) -> ApiKeyAuth {
    let owned: Vec<String> = env_vars.into_iter().map(str::to_string).collect();
    flexible_api_key_auth(name, owned)
}

/// GitHub Copilot API-key / env auth: accepts a Copilot session token **or** a GitHub
/// OAuth/PAT and exchanges the latter via `/copilot_internal/v2/token`.
pub fn github_copilot_api_key_auth() -> ApiKeyAuth {
    let env_vars = vec!["COPILOT_GITHUB_TOKEN".to_string(), "GITHUB_TOKEN".to_string()];
    ApiKeyAuth {
        name: "GitHub Copilot token".to_string(),
        resolve: Arc::new(move |input: AuthResolveInput| {
            let env_vars = env_vars.clone();
            Box::pin(async move {
                let mut raw: Option<String> = None;
                let mut source = String::new();
                if let Some(key) = input.credential.as_ref().and_then(|c| c.key.clone())
                    && !key.is_empty()
                    && let Some(resolved) = resolve_key_template(&key, input.ctx.as_ref()).await
                {
                    raw = Some(resolved);
                    source = "stored credential".into();
                }
                if raw.is_none()
                    && let Some(cred) = &input.credential
                    && let Some(ref env) = cred.env
                {
                    for var_name in env.keys() {
                        if let Some(value) = input.ctx.env(var_name).await
                            && !value.is_empty()
                        {
                            raw = Some(value);
                            source = format!("env:{var_name}");
                            break;
                        }
                    }
                }
                if raw.is_none() {
                    for var in &env_vars {
                        if let Some(value) = input.ctx.env(var).await
                            && !value.is_empty()
                        {
                            raw = Some(value);
                            source = var.clone();
                            break;
                        }
                    }
                }
                let token = raw?;
                match crate::auth::oauth::ensure_copilot_session_token(&token, None).await {
                    Ok(session) => {
                        let base = crate::auth::oauth::get_github_copilot_base_url(Some(&session), None);
                        Some(AuthResult {
                            auth: ModelAuth {
                                api_key: Some(session),
                                headers: None,
                                base_url: Some(base),
                            },
                            env: None,
                            source: Some(source),
                        })
                    }
                    Err(e) => {
                        log::warn!("GitHub Copilot token exchange failed: {e:#}");
                        None
                    }
                }
            })
        }),
        login: None,
    }
}

/// API-key auth that succeeds with an empty key when no env/credential is set.
///
/// Used for local / self-hosted OpenAI-compatible endpoints (Ollama, LM Studio, …)
/// that typically ignore or accept a dummy `Authorization` header.
pub fn optional_env_api_key_auth(name: impl Into<String>, env_vars: Vec<String>) -> ApiKeyAuth {
    flexible_api_key_auth_with_options(name, env_vars, true)
}

/// API-key auth with runtime-owned env var names (for disk-only / custom providers).
///
/// Resolution order: stored credential key → credential env map → process env vars.
pub fn flexible_api_key_auth(name: impl Into<String>, env_vars: Vec<String>) -> ApiKeyAuth {
    flexible_api_key_auth_with_options(name, env_vars, false)
}

fn flexible_api_key_auth_with_options(
    name: impl Into<String>,
    env_vars: Vec<String>,
    allow_missing: bool,
) -> ApiKeyAuth {
    let name = name.into();
    ApiKeyAuth {
        name: name.clone(),
        resolve: Arc::new(move |input: AuthResolveInput| {
            let env_vars = env_vars.clone();
            Box::pin(async move {
                if let Some(key) = input.credential.as_ref().and_then(|c| c.key.clone())
                    && !key.is_empty()
                {
                    // Interpolate `$VAR` / `${VAR}` templates. Unresolvable references
                    // make the value unresolved — fall through to env map / env vars.
                    if let Some(resolved) = resolve_key_template(&key, input.ctx.as_ref()).await {
                        return Some(AuthResult {
                            auth: ModelAuth {
                                api_key: Some(resolved),
                                headers: None,
                                base_url: None,
                            },
                            env: None,
                            source: Some("stored credential".to_string()),
                        });
                    }
                }
                // Check the credential's embedded env map (from env-ref entries).
                if let Some(cred) = &input.credential
                    && let Some(ref env) = cred.env
                {
                    for var_name in env.keys() {
                        if let Some(value) = input.ctx.env(var_name).await
                            && !value.is_empty()
                        {
                            return Some(AuthResult {
                                auth: ModelAuth {
                                    api_key: Some(value),
                                    headers: None,
                                    base_url: None,
                                },
                                env: None,
                                source: Some(format!("env:{var_name}")),
                            });
                        }
                    }
                }
                for var in &env_vars {
                    if let Some(value) = input.ctx.env(var).await
                        && !value.is_empty()
                    {
                        return Some(AuthResult {
                            auth: ModelAuth {
                                api_key: Some(value),
                                headers: None,
                                base_url: None,
                            },
                            env: None,
                            source: Some(var.clone()),
                        });
                    }
                }
                if allow_missing {
                    // Empty bearer — local OpenAI-compatible servers often ignore it.
                    return Some(AuthResult {
                        auth: ModelAuth {
                            api_key: Some(String::new()),
                            headers: None,
                            base_url: None,
                        },
                        env: None,
                        source: Some("no-auth (optional)".to_string()),
                    });
                }
                None
            })
        }),
        login: if allow_missing {
            None
        } else {
            Some(Arc::new(move |callbacks: Arc<dyn AuthLoginCallbacks>| {
                let name = name.clone();
                Box::pin(async move {
                    let key = callbacks
                        .prompt(super::types::AuthPrompt::Secret {
                            message: format!("Enter {name}"),
                            placeholder: None,
                        })
                        .await?;
                    Ok(ApiKeyCredential::new(key))
                })
            }))
        },
    }
}

/// True when a base URL points at a local/self-hosted endpoint that usually needs no cloud API key.
pub fn is_local_or_loopback_base_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.contains("localhost")
        || lower.contains("127.0.0.1")
        || lower.contains("[::1]")
        || lower.contains("0.0.0.0")
        || lower.starts_with("http://192.168.")
        || lower.starts_with("http://10.")
        || lower.starts_with("http://172.16.")
        || lower.starts_with("http://172.17.")
        || lower.starts_with("http://172.18.")
        || lower.starts_with("http://172.19.")
        || lower.starts_with("http://172.2")
        || lower.starts_with("http://172.3")
}

// ---------------------------------------------------------------------------
// API-key templates ($VAR / ${VAR} interpolation, Pi-compatible)
// ---------------------------------------------------------------------------

/// One segment of an API-key template.
#[derive(Debug, PartialEq)]
enum KeyTemplateSegment {
    Literal(String),
    Variable(String),
}

fn is_var_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_var_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Split a stored key template into literal/variable segments (Pi-compatible):
///
/// - `$NAME` resolves environment variable `NAME` (`$FOO_BAR` → `FOO_BAR`)
/// - `${NAME}suffix` resolves `NAME` followed by literal text
/// - `$$` escapes to a literal `$`
/// - `$` followed by anything else is treated literally
fn scan_key_template(value: &str) -> Vec<KeyTemplateSegment> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut iter = value.chars().peekable();

    while let Some(c) = iter.next() {
        if c != '$' {
            literal.push(c);
            continue;
        }
        match iter.peek().copied() {
            Some('$') => {
                iter.next();
                literal.push('$');
            }
            Some('{') => {
                iter.next();
                let mut name = String::new();
                let mut terminated = false;
                for inner in iter.by_ref() {
                    if inner == '}' {
                        terminated = true;
                        break;
                    }
                    name.push(inner);
                }
                let valid_name = !name.is_empty()
                    && is_var_start(name.chars().next().expect("non-empty"))
                    && name.chars().all(is_var_char);
                if terminated && valid_name {
                    if !literal.is_empty() {
                        segments.push(KeyTemplateSegment::Literal(std::mem::take(&mut literal)));
                    }
                    segments.push(KeyTemplateSegment::Variable(name));
                } else {
                    // Unterminated or invalid `${…}` — keep literally, including the
                    // closing brace when one was present.
                    literal.push_str("${");
                    literal.push_str(&name);
                    if terminated {
                        literal.push('}');
                    }
                }
            }
            Some(c2) if is_var_start(c2) => {
                // Consume the peeked first character before scanning the rest.
                iter.next();
                let mut name = String::new();
                name.push(c2);
                while let Some(&next) = iter.peek() {
                    if !is_var_char(next) {
                        break;
                    }
                    iter.next();
                    name.push(next);
                }
                if !literal.is_empty() {
                    segments.push(KeyTemplateSegment::Literal(std::mem::take(&mut literal)));
                }
                segments.push(KeyTemplateSegment::Variable(name));
            }
            _ => literal.push('$'),
        }
    }
    if !literal.is_empty() {
        segments.push(KeyTemplateSegment::Literal(literal));
    }
    segments
}

/// Resolve `$VAR` / `${VAR}` references in a stored API-key template via `ctx.env`.
///
/// Returns `None` when any referenced variable is missing or empty — the value is
/// then "unresolved" and callers fall through to other credential sources
/// (matching the reference semantics: missing env vars make the value unresolved).
async fn resolve_key_template(value: &str, ctx: &dyn AuthContext) -> Option<String> {
    let segments = scan_key_template(value);
    let mut out = String::with_capacity(value.len());
    for segment in segments {
        match segment {
            KeyTemplateSegment::Literal(text) => out.push_str(&text),
            KeyTemplateSegment::Variable(name) => {
                let resolved = ctx.env(&name).await.filter(|v| !v.is_empty())?;
                out.push_str(&resolved);
            }
        }
    }
    Some(out)
}

pub fn lazy_oauth(name: impl Into<String>, load: OAuthLoader) -> OAuthAuth {
    let name = name.into();
    let inner: Arc<tokio::sync::Mutex<Option<Arc<OAuthAuth>>>> = Arc::new(tokio::sync::Mutex::new(None));
    let load_login = load.clone();
    let load_refresh = load.clone();
    let load_to_auth = load;
    let inner_login = inner.clone();
    let inner_refresh = inner.clone();
    let inner_to_auth = inner;

    OAuthAuth {
        name: name.clone(),
        login: Arc::new(move |callbacks, identity| {
            let inner = inner_login.clone();
            let load = load_login.clone();
            Box::pin(async move {
                let auth = loaded(&inner, &load).await;
                (auth.login)(callbacks, identity).await
            })
        }),
        refresh: Arc::new(move |credential| {
            let inner = inner_refresh.clone();
            let load = load_refresh.clone();
            Box::pin(async move {
                let auth = loaded(&inner, &load).await;
                (auth.refresh)(credential).await
            })
        }),
        to_auth: Arc::new(move |credential| {
            let inner = inner_to_auth.clone();
            let load = load_to_auth.clone();
            Box::pin(async move {
                let auth = loaded(&inner, &load).await;
                (auth.to_auth)(credential).await
            })
        }),
    }
}

pub type OAuthLoader = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = OAuthAuth> + Send>> + Send + Sync>;

async fn loaded(slot: &Arc<tokio::sync::Mutex<Option<Arc<OAuthAuth>>>>, load: &OAuthLoader) -> Arc<OAuthAuth> {
    let mut guard = slot.lock().await;
    if guard.is_none() {
        *guard = Some(Arc::new(load().await));
    }
    guard.clone().unwrap()
}

pub fn auth_model_provider(model: &AuthModel) -> &str {
    match model {
        AuthModel::Chat(m) => &m.provider,
        AuthModel::Images(m) => &m.provider,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::types::BoxFuture;
    use std::collections::HashMap;

    fn literals(value: &str) -> Vec<KeyTemplateSegment> {
        scan_key_template(value)
    }

    #[test]
    fn template_plain_literal_has_no_variables() {
        assert_eq!(literals("sk-ant-123"), vec![KeyTemplateSegment::Literal("sk-ant-123".into())]);
    }

    #[test]
    fn template_bare_variable_consumes_longest_name() {
        // `$FOO_BAR` resolves variable FOO_BAR, not FOO + "_BAR".
        assert_eq!(literals("$FOO_BAR"), vec![KeyTemplateSegment::Variable("FOO_BAR".into())]);
    }

    #[test]
    fn template_braced_variable_with_literal_suffix() {
        assert_eq!(
            literals("${KEY_PREFIX}_KEY"),
            vec![
                KeyTemplateSegment::Variable("KEY_PREFIX".into()),
                KeyTemplateSegment::Literal("_KEY".into()),
            ]
        );
    }

    #[test]
    fn template_double_dollar_escapes() {
        assert_eq!(literals("$$literal"), vec![KeyTemplateSegment::Literal("$literal".into())]);
    }

    #[test]
    fn template_stray_dollar_is_literal() {
        // `$` before a digit/space/end is not a valid variable start.
        assert_eq!(
            literals("pa$$word $1 $"),
            vec![KeyTemplateSegment::Literal("pa$word $1 $".into())]
        );
    }

    #[test]
    fn template_unterminated_brace_stays_literal() {
        assert_eq!(
            literals("prefix${NO_CLOSE"),
            vec![KeyTemplateSegment::Literal("prefix${NO_CLOSE".into())]
        );
    }

    #[test]
    fn template_empty_braces_stay_literal() {
        assert_eq!(literals("a${}b"), vec![KeyTemplateSegment::Literal("a${}b".into())]);
    }

    #[test]
    fn template_adjacent_variables_both_resolve() {
        assert_eq!(
            literals("$A$B"),
            vec![
                KeyTemplateSegment::Variable("A".into()),
                KeyTemplateSegment::Variable("B".into()),
            ]
        );
    }

    struct MockCtx {
        vars: HashMap<String, String>,
    }

    impl AuthContext for MockCtx {
        fn env<'a>(&'a self, name: &'a str) -> BoxFuture<'a, Option<String>> {
            let value = self.vars.get(name).cloned();
            Box::pin(async move { value })
        }

        fn file_exists<'a>(&'a self, _path: &'a str) -> BoxFuture<'a, bool> {
            Box::pin(async move { false })
        }
    }

    fn ctx(vars: &[(&str, &str)]) -> MockCtx {
        MockCtx {
            vars: vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    #[tokio::test]
    async fn resolve_interpolates_variables_and_literals() {
        let mock = ctx(&[("KEY_PREFIX", "sk-sp"), ("REGION", "sg")]);
        let resolved = resolve_key_template("${KEY_PREFIX}_${REGION}-tail", &mock).await;
        assert_eq!(resolved.as_deref(), Some("sk-sp_sg-tail"));
    }

    #[tokio::test]
    async fn resolve_missing_variable_is_unresolved() {
        let mock = ctx(&[]);
        assert_eq!(resolve_key_template("$MISSING_KEY", &mock).await, None);
    }

    #[tokio::test]
    async fn resolve_empty_variable_is_unresolved() {
        let mock = ctx(&[("EMPTY", "")]);
        assert_eq!(resolve_key_template("$EMPTY", &mock).await, None);
    }
}
