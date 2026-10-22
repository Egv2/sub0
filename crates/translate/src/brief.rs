//! Localization brief: one call per source transcript, shared by every target
//! language. Style profiles are plain markdown files, so supporting a new kind
//! of content means dropping a file into `prompts/styles/`.

use std::path::Path;

use serde::{Deserialize, Serialize};
use subtitler_core::model::Track;

use crate::provider::LlmProvider;

pub const BRIEF_PROMPT_VERSION: &str = "2";
const BRIEF_SYSTEM_PROMPT: &str = include_str!("../../../prompts/brief.system.md");

const SAMPLE_ALL_UP_TO: usize = 400;
const SAMPLE_HEAD: usize = 160;
const SAMPLE_MID: usize = 120;
const SAMPLE_TAIL: usize = 120;
const MAX_SPEAKERS: usize = 12;
const MAX_GLOSSARY: usize = 40;
const MAX_IDIOMS: usize = 30;
const ATTEMPTS: u32 = 2;
const PROFANITY: &str = "profanity";

const BUILTIN_STYLES: &[(&str, &str)] = &[
    ("news", include_str!("../../../prompts/styles/news.md")),
    ("documentary", include_str!("../../../prompts/styles/documentary.md")),
    ("educational", include_str!("../../../prompts/styles/educational.md")),
    ("interview", include_str!("../../../prompts/styles/interview.md")),
    ("vlog", include_str!("../../../prompts/styles/vlog.md")),
    ("comedy", include_str!("../../../prompts/styles/comedy.md")),
    ("drama", include_str!("../../../prompts/styles/drama.md")),
    ("kids", include_str!("../../../prompts/styles/kids.md")),
    ("profanity", include_str!("../../../prompts/styles/profanity.md")),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct StyleProfile {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub when: String,
    pub text: String,
}

impl StyleProfile {
    /// Files start with `<!-- when: ... -->`; the rest is the guidance.
    fn parse(name: &str, raw: &str) -> Self {
        let raw = raw.trim();
        let (when, text) = match raw.strip_prefix("<!--").and_then(|r| r.split_once("-->")) {
            Some((head, rest)) => (
                head.trim().strip_prefix("when:").unwrap_or("").trim().to_string(),
                rest.trim().to_string(),
            ),
            None => (String::new(), raw.to_string()),
        };
        Self { name: name.to_string(), when, text }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StyleCatalog {
    profiles: Vec<StyleProfile>,
}

impl StyleCatalog {
    pub fn builtin() -> Self {
        Self {
            profiles: BUILTIN_STYLES
                .iter()
                .map(|(name, raw)| StyleProfile::parse(name, raw))
                .collect(),
        }
    }

    /// Every `*.md` in `dir`, or the built-in set when there is none.
    pub fn load(dir: &Path) -> Self {
        let mut profiles: Vec<StyleProfile> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension()? != "md" {
                    return None;
                }
                let name = path.file_stem()?.to_str()?.to_lowercase();
                let raw = std::fs::read_to_string(&path).ok()?;
                Some(StyleProfile::parse(&name, &raw))
            })
            .collect();
        if profiles.is_empty() {
            return Self::builtin();
        }
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        Self { profiles }
    }

    pub fn get(&self, name: &str) -> Option<&StyleProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    fn menu(&self) -> String {
        self.profiles
            .iter()
            .map(|p| format!("- {}: {}", p.name, p.when))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Speaker {
    pub name: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct GlossaryEntry {
    pub term: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Idiom {
    pub source: String,
    pub meaning: String,
}

/// Saved as `brief.json` together with the resolved profile texts, so it can
/// be hand-edited and reused without another call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Brief {
    pub media_type: String,
    pub styles: Vec<String>,
    pub style_guide: String,
    pub register: String,
    pub audience: String,
    pub profanity: String,
    pub humor: String,
    pub tone: String,
    pub setting: String,
    pub speakers: Vec<Speaker>,
    pub glossary: Vec<GlossaryEntry>,
    pub idioms: Vec<Idiom>,
    pub notes: String,
    pub profiles: Vec<StyleProfile>,
}

impl Brief {
    fn resolve(mut self, catalog: &StyleCatalog) -> Self {
        self.profanity = self.profanity.trim().to_lowercase();
        self.audience = self.audience.trim().to_lowercase();
        self.speakers.truncate(MAX_SPEAKERS);
        self.glossary.truncate(MAX_GLOSSARY);
        self.idioms.truncate(MAX_IDIOMS);

        let mut styles: Vec<String> = self
            .styles
            .iter()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| catalog.get(s).is_some())
            .collect();
        if self.profanity == "strong" && catalog.get(PROFANITY).is_some() {
            styles.push(PROFANITY.into());
        }
        // Children's content never gets the profanity profile, whatever the model picked.
        if self.audience == "children" || styles.iter().any(|s| s == "kids") {
            styles.retain(|s| s != PROFANITY);
        }
        let mut seen = std::collections::HashSet::new();
        styles.retain(|s| seen.insert(s.clone()));

        self.profiles = styles
            .iter()
            .filter_map(|s| catalog.get(s).cloned())
            .map(|p| StyleProfile { when: String::new(), ..p })
            .collect();
        self.styles = styles;
        self
    }

    pub fn localization_block(&self) -> String {
        let mut facts = serde_json::to_value(self).unwrap_or_default();
        if let Some(map) = facts.as_object_mut() {
            for key in ["styles", "style_guide", "profiles"] {
                map.remove(key);
            }
        }
        let facts = serde_json::to_string_pretty(&facts).unwrap_or_default();
        let mut out = format!("\nLocalization brief (from the full transcript):\n```json\n{facts}\n```\n");
        if !self.style_guide.trim().is_empty() {
            out.push_str(&format!("\nStyle guide for this content:\n{}\n", self.style_guide.trim()));
        }
        for p in &self.profiles {
            out.push_str(&format!("\nStyle profile ({}):\n{}\n", p.name, p.text));
        }
        out
    }
}

fn sample_lines(source: &Track) -> String {
    let n = source.cues.len();
    let picked: std::collections::BTreeSet<usize> = if n <= SAMPLE_ALL_UP_TO {
        (0..n).collect()
    } else {
        let mid = n / 2 - SAMPLE_MID / 2;
        (0..SAMPLE_HEAD)
            .chain(mid..mid + SAMPLE_MID)
            .chain(n - SAMPLE_TAIL..n)
            .collect()
    };
    picked
        .iter()
        .map(|&i| format!("{}: {}", source.cues[i].id, source.cues[i].text))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse(content: &str, catalog: &StyleCatalog) -> Option<Brief> {
    let s = content.trim();
    let s = s.strip_prefix("```").map_or(s, |r| r.trim_start_matches("json").trim());
    let s = s.strip_suffix("```").map_or(s, str::trim_end);
    serde_json::from_str::<Brief>(s).ok().map(|b| b.resolve(catalog))
}

/// Returns `None` if the model never produced a usable brief; translation
/// then simply runs without one.
pub async fn generate_brief(
    provider: &dyn LlmProvider,
    model: &str,
    source: &Track,
    catalog: &StyleCatalog,
    cache: Option<&subtitler_store::Store>,
) -> Option<Brief> {
    if source.cues.is_empty() {
        return None;
    }
    let user = format!(
        "Available style profiles:\n{}\n\nSource language: {}\n\n{}",
        catalog.menu(),
        source.lang,
        sample_lines(source)
    );
    let key = subtitler_store::Store::cache_key(&[
        "brief",
        BRIEF_PROMPT_VERSION,
        &subtitler_store::Store::hash_hex(BRIEF_SYSTEM_PROMPT.as_bytes()),
        model,
        &user,
    ]);
    if let Some(store) = cache {
        if let Ok(Some(entry)) = store.llm_get(&key).await {
            if let Some(brief) = parse(&entry.response, catalog) {
                return Some(brief);
            }
        }
    }
    for attempt in 0..ATTEMPTS {
        match crate::chat_json_with_retries(provider, model, BRIEF_SYSTEM_PROMPT, &user).await {
            Ok((content, usage)) => {
                let (tin, tout) = usage.map(|u| (u.input_tokens, u.output_tokens)).unwrap_or((0, 0));
                if let Some(store) = cache {
                    let _ = store.usage("brief", provider.name(), model, tin, tout).await;
                }
                if let Some(brief) = parse(&content, catalog) {
                    if let Some(store) = cache {
                        let _ = store.llm_put(&key, &content, tin, tout).await;
                    }
                    return Some(brief);
                }
                eprintln!("[brief] unusable response (attempt {}/{ATTEMPTS})", attempt + 1);
            }
            Err(e) => {
                eprintln!("[brief] call failed ({}): {e}", provider.name());
                return None;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brief(json: &str) -> Brief {
        parse(json, &StyleCatalog::builtin()).unwrap()
    }

    #[test]
    fn content_outside_the_catalog_still_gets_a_guide() {
        let b = brief(
            r#"{"media_type": "sports commentary", "styles": ["sports", "Interview"],
                "style_guide": "Fast, punchy play-by-play."}"#,
        );
        assert_eq!(b.styles, vec!["interview"]);
        let block = b.localization_block();
        assert!(block.contains("Fast, punchy play-by-play."));
        assert!(block.contains("Style profile (interview)"));
        assert!(!block.contains("\"profiles\""));
    }

    #[test]
    fn strong_profanity_adds_the_profile_except_for_children() {
        let b = brief(r#"{"styles": ["comedy"], "profanity": "strong", "audience": "adult"}"#);
        assert_eq!(b.styles, vec!["comedy", "profanity"]);
        let kids = brief(r#"{"styles": ["kids", "profanity"], "profanity": "strong"}"#);
        assert_eq!(kids.styles, vec!["kids"]);
    }

    #[test]
    fn profile_files_parse_their_when_header() {
        let p = StyleProfile::parse("gaming", "<!-- when: game streams -->\nKeep gamer slang.");
        assert_eq!(p.when, "game streams");
        assert_eq!(p.text, "Keep gamer slang.");
        assert!(StyleCatalog::builtin().get("profanity").is_some());
    }

    #[test]
    fn catalog_loads_extra_profiles_from_disk() {
        let dir = std::env::temp_dir().join(format!("styles-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sermon.md"), "<!-- when: religious talks -->\nReverent.").unwrap();
        let catalog = StyleCatalog::load(&dir);
        assert_eq!(catalog.get("sermon").unwrap().when, "religious talks");
        assert!(StyleCatalog::load(&dir.join("missing")).get("news").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
