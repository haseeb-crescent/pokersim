//! Claude for plain-English explanations and the "ask why" chat.
//!
//! The engine computes every number; the model only turns the fact sheet into sentences.
//!
//! Backends, in order of preference:
//! 1. The local `claude` CLI in print mode, using the Claude subscription it is logged in to.
//!    It runs isolated: from an empty working directory, with an allow-listed environment (no
//!    `ANTHROPIC_*` or host-session variables, so it never bills an API key), `--safe-mode`
//!    (no CLAUDE.md, hooks, plugins, skills or MCP servers), `--tools ""`, an empty strict MCP
//!    config and the coach's own `--system-prompt`. "Ask why" follow-ups continue the same
//!    conversation with `--resume <session_id>`.
//! 2. The Messages API with `ANTHROPIC_API_KEY` (from `.env`).
//! 3. Neither: the coach shows fact sheets without prose.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;

pub const API_MODEL: &str = "claude-sonnet-5-5";
const CLI_MODEL: &str = "sonnet";
const DEFAULT_BASE: &str = "https://api.anthropic.com";
const VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const CLI_TIMEOUT: Duration = Duration::from_secs(150);

const EXPLAIN_SYSTEM: &str = "You are a poker coach explaining one decision to a student who is studying game theory optimal (GTO) and exploitative no-limit hold'em.

You receive a JSON fact sheet computed by a solver and an equity engine. Write 2 to 5 plain sentences for the student.

Rules:
- Use only numbers that appear in the facts, quoted as written (percentages are already computed). Never calculate, estimate or derive a new number, and never compare two numbers yourself: use the statements in facts.comparisons. If a comparison you want is not there, describe it in words without numbers.
- Explain why the solver plays this exact hand the way it does, using the most relevant facts (equity against the range versus the pot odds needed, range or nut advantage, blockers, minimum defense frequency, stack-to-pot ratio, the solver's mix).
- If the student's action differs from the solver's preference, say what it cost using ev_loss_bb and why it is worse. If the student chose an action the solver also uses, say so.
- If off_chart is present, the student's action is not in the preflop chart at all: say what the chart does with this hand instead, and that the rest of the hand has no solver grades.
- If kind is \"unsolved\", give the reason from unsolved_reason in a few words, then reason only from pot odds and the equity against estimated ranges.
- If kind is \"preflop_chart\", the charts carry no EVs: talk about frequencies, not EV.
- If an exploit view is present, add one sentence on how to adjust against that opponent type.
- Plain prose only: no headings, no bullet points, no preamble, no sign-off.
- You have no tools. Answer from the fact sheet only.";

const ASK_SYSTEM: &str = "You are a poker coach answering a student's follow-up questions about one hand they played. The first message contains the hand's facts as JSON, computed by a solver and an equity engine.

Rules:
- Ground every claim in the facts. Use only numbers that appear in them, quoted as written; never calculate, estimate or compare numbers yourself (use the statements under comparisons). If a question needs a number that is not in the facts, say you do not have it and explain the idea qualitatively.
- Keep answers short and concrete: a few sentences, plain language, define jargon briefly when you use it.
- If the student asks about something the facts do not cover (another street, a different hand), say what the facts do cover and reason from general principles, clearly labelled as general.
- You have no tools. Answer from the facts and the conversation only.";

struct Cli {
    bin: PathBuf,
    dir: PathBuf,
    env: Vec<(String, String)>,
}

struct Api {
    key: String,
    url: String,
    client: reqwest::Client,
}

pub struct Llm {
    cli: Option<Cli>,
    api: Option<Api>,
    cli_ok: AtomicBool,
    permits: tokio::sync::Semaphore,
}

/// Environment for the CLI: only what it needs to find its login and run.
fn cli_env() -> Vec<(String, String)> {
    const KEEP: &[&str] = &["HOME", "USER", "LOGNAME", "PATH", "SHELL", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE", "XDG_CONFIG_HOME", "CLAUDE_CONFIG_DIR"];
    std::env::vars().filter(|(k, _)| KEEP.contains(&k.as_str())).collect()
}

fn find_claude() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CLAUDE_BIN") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let mut candidates: Vec<PathBuf> = std::env::var("PATH").unwrap_or_default().split(':').map(|d| PathBuf::from(d).join("claude")).collect();
    for extra in [".local/bin/claude", ".claude/local/claude", ".npm-global/bin/claude"] {
        candidates.push(PathBuf::from(&home).join(extra));
    }
    candidates.push("/opt/homebrew/bin/claude".into());
    candidates.push("/usr/local/bin/claude".into());
    candidates.into_iter().find(|p| p.is_file())
}

impl Cli {
    /// The CLI if it is installed and logged in (`claude auth status`, no model call).
    async fn detect() -> Option<Cli> {
        let bin = find_claude()?;
        let dir = std::env::temp_dir().join("pokersim-coach");
        std::fs::create_dir_all(&dir).ok()?;
        let env = cli_env();
        let out = tokio::time::timeout(
            Duration::from_secs(20),
            tokio::process::Command::new(&bin).args(["auth", "status"]).current_dir(&dir).env_clear().envs(env.iter().cloned()).output(),
        )
        .await
        .ok()?
        .ok()?;
        let status: Value = serde_json::from_slice(&out.stdout).ok()?;
        if status.get("loggedIn").and_then(|v| v.as_bool()) != Some(true) {
            return None;
        }
        Some(Cli { bin, dir, env })
    }

    /// One print-mode call. Returns (text, session id).
    async fn call(&self, system: &str, prompt: &str, resume: Option<&str>) -> anyhow::Result<(String, String)> {
        let mut cmd = tokio::process::Command::new(&self.bin);
        cmd.current_dir(&self.dir)
            .env_clear()
            .envs(self.env.iter().cloned())
            .args(["-p", "--model", CLI_MODEL, "--output-format", "json"])
            .args(["--safe-mode", "--tools", "", "--strict-mcp-config", "--mcp-config", r#"{"mcpServers":{}}"#])
            .args(["--disable-slash-commands", "--permission-prompts", "none"])
            .args(["--system-prompt", system]);
        if let Some(id) = resume {
            cmd.args(["--resume", id]);
        }
        cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
        let mut child = cmd.spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(prompt.as_bytes()).await?;
        }
        let out = tokio::time::timeout(CLI_TIMEOUT, child.wait_with_output()).await.map_err(|_| anyhow::anyhow!("claude CLI timed out"))??;
        let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| {
            let err = String::from_utf8_lossy(&out.stderr);
            anyhow::anyhow!("claude CLI failed: {}", err.lines().last().unwrap_or("no output"))
        })?;
        let text = v.get("result").and_then(|r| r.as_str()).unwrap_or("").trim().to_string();
        if v.get("is_error").and_then(|e| e.as_bool()).unwrap_or(false) || text.is_empty() {
            anyhow::bail!("claude CLI error: {}", if text.is_empty() { v.get("subtype").and_then(|s| s.as_str()).unwrap_or("unknown") } else { &text });
        }
        let session = v.get("session_id").and_then(|s| s.as_str()).unwrap_or("").to_string();
        Ok((text, session))
    }
}

impl Api {
    fn from_env() -> Option<Api> {
        let key = std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.trim().is_empty())?;
        let client = reqwest::Client::builder().timeout(Duration::from_secs(120)).build().ok()?;
        let base = std::env::var("ANTHROPIC_BASE_URL").ok().filter(|b| !b.is_empty()).unwrap_or_else(|| DEFAULT_BASE.to_string());
        Some(Api { key, url: format!("{}/v1/messages", base.trim_end_matches('/')), client })
    }

    async fn call(&self, system: &str, messages: Vec<Value>, max_tokens: u32, cache: bool) -> anyhow::Result<String> {
        let mut body = json!({
            "model": API_MODEL,
            "max_tokens": max_tokens,
            "system": system,
            "messages": messages,
            "output_config": {"effort": "low"},
            "fallbacks": "default",
        });
        if cache {
            body["cache_control"] = json!({"type": "ephemeral"});
        }
        let resp = self
            .client
            .post(&self.url)
            .header("x-api-key", &self.key)
            .header("anthropic-version", VERSION)
            .header("anthropic-beta", FALLBACK_BETA)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let v: Value = resp.json().await?;
        if !status.is_success() {
            let msg = v.pointer("/error/message").and_then(|m| m.as_str()).unwrap_or("unknown error");
            anyhow::bail!("Claude API {status}: {msg}");
        }
        if v.get("stop_reason").and_then(|s| s.as_str()) == Some("refusal") {
            anyhow::bail!("the model declined to answer this one");
        }
        let text: String = v
            .get("content")
            .and_then(|c| c.as_array())
            .map(|blocks| blocks.iter().filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text")).filter_map(|b| b.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join(""))
            .unwrap_or_default();
        Ok(text.trim().to_string())
    }
}

/// Result of one "ask why" turn. `chat_id` continues a CLI conversation (None for the API,
/// which is stateless and resent the history).
pub struct AskReply {
    pub text: String,
    pub chat_id: Option<String>,
}

impl Llm {
    /// Picks the backend: the logged-in `claude` CLI first, then `ANTHROPIC_API_KEY`.
    pub async fn detect() -> Option<Llm> {
        let cli = if std::env::var("POKERSIM_NO_CLI").is_ok() { None } else { Cli::detect().await };
        let api = Api::from_env();
        if cli.is_none() && api.is_none() {
            return None;
        }
        let ok = cli.is_some();
        Some(Llm { cli, api, cli_ok: AtomicBool::new(ok), permits: tokio::sync::Semaphore::new(2) })
    }

    /// "claude-cli", "api", or "none".
    pub fn via(&self) -> &'static str {
        if self.cli.is_some() && self.cli_ok.load(Ordering::Relaxed) {
            "claude-cli"
        } else if self.api.is_some() {
            "api"
        } else {
            "none"
        }
    }

    pub fn describe(&self) -> String {
        match self.via() {
            "claude-cli" => format!("Claude subscription via the claude CLI ({CLI_MODEL})"),
            "api" => format!("Claude API ({API_MODEL}, ANTHROPIC_API_KEY)"),
            _ => "off".into(),
        }
    }

    fn use_cli(&self) -> Option<&Cli> {
        self.cli.as_ref().filter(|_| self.cli_ok.load(Ordering::Relaxed))
    }

    /// Falls back to the API for good when the CLI fails and an API key is available.
    fn cli_failed(&self, e: &anyhow::Error) -> bool {
        eprintln!("claude CLI call failed: {e}");
        if self.api.is_some() {
            self.cli_ok.store(false, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// 2-5 sentences explaining a graded decision from its fact sheet.
    pub async fn explain(&self, grade: &Value) -> anyhow::Result<String> {
        let _permit = self.permits.acquire().await?;
        let facts = explain_payload(grade);
        let prompt = format!("Fact sheet:\n{}", serde_json::to_string_pretty(&facts)?);
        if let Some(cli) = self.use_cli() {
            match cli.call(EXPLAIN_SYSTEM, &prompt, None).await {
                Ok((text, _)) => return Ok(text),
                Err(e) => {
                    if !self.cli_failed(&e) {
                        return Err(e);
                    }
                }
            }
        }
        let api = self.api.as_ref().ok_or_else(|| anyhow::anyhow!("no explanation backend"))?;
        api.call(EXPLAIN_SYSTEM, vec![json!({"role": "user", "content": prompt})], 2000, false).await
    }

    /// One turn of the "ask why" chat. `history` alternates user / assistant and ends with the
    /// student's new question. With the CLI, `chat_id` resumes the conversation, so only the
    /// new question is sent.
    pub async fn ask(&self, context: &Value, history: &[(String, String)], chat_id: Option<&str>) -> anyhow::Result<AskReply> {
        let _permit = self.permits.acquire().await?;
        let question = history.last().map(|h| h.1.clone()).unwrap_or_default();
        let intro = format!("Facts for this hand:\n{}", serde_json::to_string_pretty(context)?);
        if let Some(cli) = self.use_cli() {
            let result = match chat_id.filter(|id| !id.is_empty()) {
                Some(id) => cli.call(ASK_SYSTEM, &question, Some(id)).await,
                None => {
                    // first turn (or a conversation that started on another backend)
                    let mut prompt = intro.clone();
                    if history.len() > 1 {
                        prompt.push_str("\n\nConversation so far:\n");
                        for (role, text) in &history[..history.len() - 1] {
                            prompt.push_str(&format!("{}: {}\n", if role == "assistant" { "Coach" } else { "Student" }, text));
                        }
                    }
                    prompt.push_str(&format!("\n\nQuestion: {question}"));
                    cli.call(ASK_SYSTEM, &prompt, None).await
                }
            };
            match result {
                Ok((text, session)) => return Ok(AskReply { text, chat_id: Some(session).filter(|s| !s.is_empty()) }),
                Err(e) => {
                    if !self.cli_failed(&e) {
                        return Err(e);
                    }
                }
            }
        }
        let api = self.api.as_ref().ok_or_else(|| anyhow::anyhow!("no explanation backend"))?;
        let mut messages = Vec::new();
        for (i, (role, text)) in history.iter().enumerate() {
            let content = if i == 0 { format!("{intro}\n\nQuestion: {text}") } else { text.clone() };
            messages.push(json!({"role": role, "content": content}));
        }
        let text = api.call(ASK_SYSTEM, messages, 3000, true).await?;
        Ok(AskReply { text, chat_id: None })
    }
}

fn pct(v: Option<&Value>) -> Value {
    match v.and_then(|x| x.as_f64()) {
        Some(x) => json!(format!("{:.1}%", 100.0 * x)),
        None => Value::Null,
    }
}

/// The fact sheet with every probability written as a percentage, plus the comparisons the
/// coach needs spelled out, so the model never has to compare or convert numbers itself.
fn facts_for_llm(f: &Value) -> Value {
    if f.is_null() {
        return Value::Null;
    }
    let mut out = f.clone();
    for k in ["pot_odds_needed", "equity_vs_range", "mdf", "bluff_ratio"] {
        out[k] = pct(f.get(k));
    }
    for k in ["range_advantage", "nut_advantage"] {
        if let Some(p) = f.get(k).filter(|x| !x.is_null()) {
            out[k] = json!({"hero": pct(p.get("hero")), "villain": pct(p.get("villain"))});
        }
    }
    let mut comparisons = Vec::new();
    if let (Some(eq), Some(need)) = (f.get("equity_vs_range").and_then(|x| x.as_f64()), f.get("pot_odds_needed").and_then(|x| x.as_f64())) {
        let rel = if eq >= need { "above" } else { "below" };
        comparisons.push(format!("your equity ({:.1}%) is {rel} the {:.1}% needed to call (raw equity, before position and realization)", 100.0 * eq, 100.0 * need));
    }
    for k in ["range_advantage", "nut_advantage"] {
        if let Some(p) = f.get(k) {
            if let (Some(h), Some(v)) = (p.get("hero").and_then(|x| x.as_f64()), p.get("villain").and_then(|x| x.as_f64())) {
                let who = if h > v { "you have" } else if h < v { "the villain has" } else { "nobody has" };
                let what = if k == "range_advantage" { "the range advantage (average equity)" } else { "the nut advantage (share of strong hands)" };
                comparisons.push(format!("{who} {what}: you {:.1}% vs villain {:.1}%", 100.0 * h, 100.0 * v));
            }
        }
    }
    out["comparisons"] = json!(comparisons);
    out
}

fn actions_for_llm(a: Option<&Value>) -> Value {
    let Some(list) = a.and_then(|x| x.as_array()) else { return Value::Null };
    json!(list
        .iter()
        .map(|x| json!({"action": x.get("label"), "frequency_for_this_hand": pct(x.get("freq")), "ev_bb": x.get("ev_bb"), "frequency_over_range": pct(x.get("range_freq"))}))
        .collect::<Vec<_>>())
}

/// The parts of a grade the explainer needs (drops grids and large arrays). Probabilities are
/// pre-formatted as percentages and comparisons are stated explicitly.
pub fn explain_payload(g: &Value) -> Value {
    let exploit = g.get("exploit").filter(|e| !e.is_null()).map(|e| {
        json!({"profile": e.get("profile"), "best": e.get("best"), "actions": actions_for_llm(e.get("actions")), "note": e.get("note")})
    });
    json!({
        "spot": g.get("spot"),
        "street": g.get("street"),
        "kind": g.get("kind"),
        "hero_hand": g.get("hero_hand"),
        "board": g.get("board"),
        "solver_actions_for_this_hand": actions_for_llm(g.get("actions")),
        "student_chose": g.get("chosen"),
        "student_choice_frequency": pct(g.get("chosen_freq")),
        "solver_best": g.get("best"),
        "ev_loss_bb": g.get("ev_loss_bb"),
        "grade": g.get("grade"),
        "note": g.get("note"),
        "off_chart": g.get("off_chart"),
        "unsolved_reason": g.get("unsolved_reason"),
        "facts": facts_for_llm(g.get("facts").unwrap_or(&Value::Null)),
        "exploit": exploit,
    })
}
