//! LLM translation over an id→text contract. The model never sees timecodes;
//! output is re-attached to the source cues by id.

pub mod brief;
pub mod error;
pub mod provider;
pub mod translate;

pub use error::LlmError;
pub use provider::{LlmProvider, OpenAiCompatLlm, Usage};
pub use brief::{generate_brief, Brief, StyleCatalog, StyleProfile};
pub use translate::{
    chat_json_with_retries, system_prompt, system_prompt_with, translate_track, TranslateOptions, TranslateStats,
    PROMPT_VERSION,
};

#[cfg(test)]
mod tests;
