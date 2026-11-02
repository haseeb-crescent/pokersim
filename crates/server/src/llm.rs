//! Claude (Messages API over HTTPS) for plain-English explanations and the "ask why" chat.
//!
//! The engine computes every number; the model only turns the fact sheet into sentences.
//! Model `claude-sonnet-5-5`, key from `ANTHROPIC_API_KEY` (loaded from `.env`). Refusals are
//! retried server-side via `fallbacks: "default"`; `stop_reason` is checked before reading
//! the text.

use serde_json::{json, Value};

pub const MODEL: &str = "claude-sonnet-5-5";
const DEFAULT_BASE: &str = "https://api.anthropic.com";
const VERSION: &str = "2026-10-21";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

const EXPLAIN_SYSTEM: &str = "You are a poker coach explaining one decision to a student who is studying game theory optimal (GTO) and exploitative no-limit hold'em.

You receive a JSON fact sheet computed by a solver and an equity engine. Write 2 to 5 plain sentences for the student.

Rules:
- Use only numbers that appear in the facts. Never calculate, estimate or derive a new number; if a comparison needs a number that is not given, describe it in words instead.
- Explain why the solver plays this exact hand the way it does, using the most relevant facts (equity against the range versus the pot odds needed, range or nut advantage, blockers, minimum defense frequency, stack-to-pot ratio, the solver's mix).
- If the student's action differs from the solver's preference, say what it cost using ev_loss_bb and why it is worse. If the student chose an action the solver also uses, say so.
- If kind is \"unsolved\", say multiway pots have no solver solution so there is no grade, and reason only from pot odds and equity.
- If kind is \"preflop_chart\", the charts carry no EVs: talk about frequencies, not EV.
- If an exploit view is present, add one sentence on how to adjust against that opponent type.
- Plain prose only: no headings, no bullet points, no preamble, no sign-off.";

const ASK_SYSTEM: &str = "You are a poker coach answering a student's follow-up questions about one hand they played. The first message contains the hand's facts as JSON, computed by a solver and an equity engine.

Rules:
- Ground every claim in the facts. Use only numbers that appear in them; never calculate or estimate new numbers. If a question needs a number that is not in the facts, say you do not have it and explain the idea qualitatively.
- Keep answers short and concrete: a few sentences, plain language, define jargon briefly when you use it.
- If the student asks about something the facts do not cover (another street, a different hand), say what the facts do cover and reason from general principles, clearly labelled as general.";

pub struct Llm {
    key: String,
    url: String,
    client: reqwest::Client,
}

impl Llm {
    pub fn from_env() -> Option<Llm> {
        let key = std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.trim().is_empty())?;
        let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).build().ok()?;
        let base = std::env::var("ANTHROPIC_BASE_URL").ok().filter(|b| !b.is_empty()).unwrap_or_else(|| DEFAULT_BASE.to_string());
        Some(Llm { key, url: format!("{}/v1/messages", base.trim_end_matches('/')), client })
    }

    async fn call(&self, system: &str, messages: Vec<Value>, max_tokens: u32, cache: bool) -> anyhow::Result<String> {
        let mut body = json!({
            "model": MODEL,
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

    /// 2-5 sentences explaining a graded decision from its fact sheet.
    pub async fn explain(&self, grade: &Value) -> anyhow::Result<String> {
        let facts = explain_payload(grade);
        let msg = json!({"role": "user", "content": format!("Fact sheet:\n{}", serde_json::to_string_pretty(&facts)?)});
        self.call(EXPLAIN_SYSTEM, vec![msg], 2000, false).await
    }

    /// One turn of the "ask why" chat. `history` alternates user / assistant, ending with the
    /// student's new question.
    pub async fn ask(&self, context: &Value, history: &[(String, String)]) -> anyhow::Result<String> {
        let mut messages = Vec::new();
        let intro = format!("Facts for this hand:\n{}", serde_json::to_string_pretty(context)?);
        for (i, (role, text)) in history.iter().enumerate() {
            let content = if i == 0 { format!("{intro}\n\nQuestion: {text}") } else { text.clone() };
            messages.push(json!({"role": role, "content": content}));
        }
        self.call(ASK_SYSTEM, messages, 3000, true).await
    }
}

/// The parts of a grade the explainer needs (drops grids and large arrays).
pub fn explain_payload(g: &Value) -> Value {
    json!({
        "spot": g.get("spot"),
        "street": g.get("street"),
        "kind": g.get("kind"),
        "hero_hand": g.get("hero_hand"),
        "board": g.get("board"),
        "solver_actions_for_this_hand": g.get("actions"),
        "student_chose": g.get("chosen"),
        "student_choice_frequency": g.get("chosen_freq"),
        "solver_best": g.get("best"),
        "ev_loss_bb": g.get("ev_loss_bb"),
        "grade": g.get("grade"),
        "note": g.get("note"),
        "facts": g.get("facts"),
        "exploit": g.get("exploit"),
    })
}
