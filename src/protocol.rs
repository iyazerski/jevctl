use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputDetail {
    Compact,
    Full,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationRequest {
    /// Evidence shared by every question in this batch.
    pub context: Value,
    /// Independent semantic judgments to evaluate together.
    pub questions: BTreeMap<String, Question>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Question {
    /// Return the probability that the answer is yes.
    Boolean {
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        yes_when: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        no_when: Option<String>,
    },
    /// Select one option from a caller-defined closed set.
    Select {
        prompt: String,
        options: BTreeMap<String, String>,
    },
    /// Return a position over ordered levels, starting at zero.
    Scale { prompt: String, levels: Vec<String> },
}

impl Question {
    /// Return the human-readable prompt shared by every public question kind.
    pub fn prompt(&self) -> &str {
        match self {
            Self::Boolean { prompt, .. }
            | Self::Select { prompt, .. }
            | Self::Scale { prompt, .. } => prompt,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum EvaluationOutput {
    Compact(CompactResponse),
    Full(FullResponse),
}

#[derive(Clone, Debug, Serialize)]
pub struct CompactResponse {
    pub answers: BTreeMap<String, CompactAnswer>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum CompactAnswer {
    Number(f64),
    Selection(String),
}

#[derive(Clone, Debug, Serialize)]
pub struct FullResponse {
    pub answers: BTreeMap<String, FullAnswer>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum FullAnswer {
    Boolean {
        value: f64,
    },
    Select {
        value: String,
        confidence: f64,
        probabilities: BTreeMap<String, f64>,
    },
    Scale {
        value: f64,
        confidence: f64,
        probabilities: Vec<f64>,
    },
}
