//! Model catalogue for the settings screen. OpenRouter prices are live from its
//! public `/models`; Groq has no pricing API, so its prices are a static
//! overlay and labelled as such in the UI.

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::{resolve_key, ProviderKind};

// Shown next to Groq prices so users know how old the snapshot is.
const GROQ_PRICES_CHECKED_AT: &str = "2026-10-15";
const CATALOGUE_TTL: Duration = Duration::from_secs(6 * 60 * 60);

const OR_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const OR_TRANSCRIPTION_URL: &str =
    "https://openrouter.ai/api/v1/models?output_modalities=transcription";
const GROQ_MODELS_URL: &str = "https://api.groq.com/openai/v1/models";

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("sub0/0.2")
        .build()
        .map_err(|e| e.to_string())
}

/// Prices are USD per million tokens; `per_hour` is USD per audio hour (STT only).
fn entry(
    id: String,
    name: String,
    context: u64,
    in_per_m: Option<f64>,
    out_per_m: Option<f64>,
    per_hour: Option<f64>,
    pricing: &str,
) -> Value {
    json!({
        "id": id,
        "name": name,
        "context": context,
        "in_per_m": in_per_m,
        "out_per_m": out_per_m,
        "per_hour": per_hour,
        "pricing": pricing,
    })
}

/// OpenRouter prices are per-token decimal strings, scaled to per-million here.
/// Unparsable prices become `null`, not 0: "unknown" is not "free".
fn normalise(payload: &Value, per_hour_of: &dyn Fn(&str) -> Option<f64>) -> Vec<Value> {
    let Some(list) = payload.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?.to_string();
            let name = m
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(id.as_str())
                .to_string();
            let context = m.get("context_length").and_then(|v| v.as_json_u64()).unwrap_or(0);
            let price = |key: &str| {
                m.get("pricing")
                    .and_then(|p| p.get(key))
                    .and_then(Value::as_str)
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|per_token| per_token * 1_000_000.0)
            };
            Some(entry(
                id.clone(),
                name,
                context,
                price("prompt"),
                price("completion"),
                per_hour_of(&id),
                "live",
            ))
        })
        .collect()
}

// Some payloads encode integers as floats.
trait JsonNum {
    fn as_json_u64(&self) -> Option<u64>;
}
impl JsonNum for Value {
    fn as_json_u64(&self) -> Option<u64> {
        self.as_u64()
            .or_else(|| self.as_f64().map(|f| f.max(0.0) as u64))
    }
}

fn groq_pricing(id: &str) -> (Option<f64>, Option<f64>, Option<f64>) {
    match id {
        "openai/gpt-oss-20b" => (Some(0.075), Some(0.30), None),
        "openai/gpt-oss-120b" => (Some(0.15), Some(0.60), None),
        "qwen/qwen3.8-27b" => (Some(0.80), Some(4.00), None),
        "whisper-large-v3" => (None, None, Some(0.111)),
        "whisper-large-v3-turbo" => (None, None, Some(0.04)),
        _ => (None, None, None),
    }
}

fn groq_entries(payload: &Value) -> (Vec<Value>, Vec<Value>) {
    let Some(list) = payload.get("data").and_then(Value::as_array) else {
        return (Vec::new(), Vec::new());
    };
    let (mut stt, mut llm) = (Vec::new(), Vec::new());
    for m in list {
        let Some(id) = m.get("id").and_then(Value::as_str) else {
            continue;
        };
        let is_stt = id.contains("whisper");
        // Guard/safeguard models are classifiers and can't translate.
        if !is_stt && (id.contains("guard") || id.contains("safeguard")) {
            continue;
        }
        let (in_p, out_p, hour) = groq_pricing(id);
        let known = in_p.is_some() || hour.is_some();
        let row = entry(
            id.to_string(),
            id.to_string(),
            0,
            in_p,
            out_p,
            hour,
            if known { "static" } else { "none" },
        );
        if is_stt {
            stt.push(row);
        } else {
            llm.push(row);
        }
    }
    (stt, llm)
}

async fn fetch_json(client: &reqwest::Client, url: &str, bearer: Option<&str>) -> Result<Value, String> {
    let mut req = client.get(url);
    if let Some(key) = bearer {
        req = req.bearer_auth(key);
    }
    let text = req.send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn provider(live: bool, error: Option<String>, llm: Vec<Value>, stt: Vec<Value>) -> Value {
    json!({ "live": live, "error": error, "llm": llm, "stt": stt })
}

// One failing provider doesn't sink the rest.
async fn fetch_catalogue() -> Value {
    let client = match http() {
        Ok(c) => c,
        Err(e) => {
            let empty = provider(false, Some(e.clone()), Vec::new(), Vec::new());
            return json!({ "fetched_at": now_rfc3339(), "cached": false, "providers": { "openrouter": empty.clone(), "groq": empty } });
        }
    };

    let (llm_res, stt_res) = tokio::join!(
        fetch_json(&client, OR_MODELS_URL, None),
        fetch_json(&client, OR_TRANSCRIPTION_URL, None),
    );
    let or_llm = match &llm_res {
        Ok(v) => normalise(v, &|_| None),
        Err(_) => Vec::new(),
    };
    let or_stt = match &stt_res {
        Ok(v) => normalise(v, &|_| None),
        Err(_) => Vec::new(),
    };
    let openrouter = match (&llm_res, &stt_res) {
        (Ok(_), Ok(_)) => provider(true, None, or_llm, or_stt),
        (Err(e), Ok(_)) => provider(false, Some(e.clone()), or_llm, or_stt),
        (Ok(_), Err(e)) => provider(false, Some(e.clone()), or_llm, or_stt),
        (Err(a), Err(b)) => provider(false, Some(format!("{a} / {b}")), or_llm, or_stt),
    };

    let groq = match resolve_key(ProviderKind::Groq) {
        None => provider(false, Some("Groq API anahtarı yok".into()), Vec::new(), Vec::new()),
        Some(key) => match fetch_json(&client, GROQ_MODELS_URL, Some(&key)).await {
            Ok(v) => {
                let (stt, llm) = groq_entries(&v);
                provider(true, None, llm, stt)
            }
            Err(e) => provider(false, Some(e), Vec::new(), Vec::new()),
        },
    };

    json!({
        "fetched_at": now_rfc3339(),
        "cached": false,
        "groq_prices_checked_at": GROQ_PRICES_CHECKED_AT,
        "providers": { "openrouter": openrouter, "groq": groq },
    })
}

// Avoids a date-time dependency for one timestamp.
fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    let tod = secs % 86_400;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
}

fn cache() -> &'static Mutex<Option<(Instant, Value)>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, Value)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Cached for [`CATALOGUE_TTL`]. Provider failures are reported inside the
/// payload so the UI can still show the other provider.
pub async fn get_catalogue(refresh: bool) -> Value {
    if !refresh {
        if let Ok(guard) = cache().lock() {
            if let Some((at, payload)) = guard.as_ref() {
                if at.elapsed() < CATALOGUE_TTL {
                    let mut hit = payload.clone();
                    hit["cached"] = json!(true);
                    return hit;
                }
            }
        }
    }
    let payload = fetch_catalogue().await;
    if let Ok(mut guard) = cache().lock() {
        *guard = Some((Instant::now(), payload.clone()));
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    // The per-token -> per-million conversion has been wrong before.
    const OR_FIXTURE: &str = r#"{"data":[
        {"id":"deepseek/deepseek-v4.1-flash","name":"DeepSeek: V4.1 Flash","context_length":1048576,
         "pricing":{"prompt":"0.00000001584","completion":"0.000000891"}},
        {"id":"vendor/free-model","name":"Free","context_length":200000,
         "pricing":{"prompt":"0","completion":"0"}},
        {"id":"vendor/no-ctx","pricing":{"prompt":"0.0001","completion":"0.0002"}}
    ]}"#;

    #[test]
    fn normalise_converts_per_token_prices_to_per_million() {
        let rows = normalise(&serde_json::from_str(OR_FIXTURE).unwrap(), &|_| None);
        assert_eq!(rows.len(), 3);

        let first = &rows[0];
        assert_eq!(first["id"], "deepseek/deepseek-v4.1-flash");
        assert_eq!(first["context"], 1048576);
        assert_eq!(first["pricing"], "live");
        let approx = |a: &Value, b: f64| {
            assert!((a.as_f64().unwrap() - b).abs() < 1e-9, "{a} != {b}");
        };
        approx(&first["in_per_m"], 0.01584);
        approx(&first["out_per_m"], 0.891);

        // A free model is still live data, with a real zero rather than null.
        assert_eq!(rows[1]["in_per_m"], 0.0);
        assert_eq!(rows[1]["pricing"], "live");

        // Missing context falls back to 0 instead of failing the whole list.
        assert_eq!(rows[2]["context"], 0);
        assert_eq!(rows[2]["name"], "vendor/no-ctx");
        approx(&rows[2]["in_per_m"], 100.0);
    }

    #[test]
    fn normalise_tolerates_garbage_and_empty_payloads() {
        assert!(normalise(&serde_json::json!({}), &|_| None).is_empty());
        assert!(normalise(&serde_json::json!({"data": "nope"}), &|_| None).is_empty());
        // Unparsable price becomes null ("unknown"), never a silent 0.
        let rows = normalise(
            &serde_json::json!({"data":[{"id":"x","pricing":{"prompt":"abc"}}]}),
            &|_| None,
        );
        assert!(rows[0]["in_per_m"].is_null());
    }

    #[test]
    fn groq_entries_split_speech_and_label_prices_honestly() {
        let payload = serde_json::json!({"data":[
            {"id":"whisper-large-v3"},
            {"id":"openai/gpt-oss-20b"},
            {"id":"some/new-model"}
        ]});
        let (stt, llm) = groq_entries(&payload);
        assert_eq!(stt.len(), 1);
        assert_eq!(stt[0]["id"], "whisper-large-v3");
        assert_eq!(stt[0]["per_hour"], 0.111);
        assert_eq!(stt[0]["pricing"], "static", "Groq prices are a dated snapshot");
        assert_eq!(llm.len(), 2);
        assert_eq!(llm[0]["in_per_m"], 0.075);
        assert_eq!(llm[0]["pricing"], "static");
        // Unknown id must not inherit a price from the overlay.
        assert_eq!(llm[1]["pricing"], "none");
        assert!(llm[1]["in_per_m"].is_null());
    }

    #[test]
    fn rfc3339_is_well_formed_and_utc() {
        let stamp = now_rfc3339();
        assert!(stamp.ends_with('Z'), "{stamp}");
        assert_eq!(stamp.len(), 20, "{stamp}");
        assert_eq!(&stamp[4..5], "-");
        assert_eq!(&stamp[10..11], "T");
    }
}
