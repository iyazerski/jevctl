use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputDetail {
    #[default]
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
    /// Compact returns scalar answers; full adds normalized probability distributions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<OutputDetail>,
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

impl EvaluationOutput {
    /// Format the compact answer projection as a JSON string with minimal allocations.
    pub fn to_compact_json_string(&self) -> Result<String, serde_json::Error> {
        match self {
            Self::Compact(response) => serde_json::to_string(response),
            Self::Full(response) => {
                let compact = CompactResponseRef {
                    answers: response
                        .answers
                        .iter()
                        .map(|(id, answer)| {
                            let answer = match answer {
                                FullAnswer::Boolean { value } | FullAnswer::Scale { value, .. } => {
                                    CompactAnswerRef::Number(*value)
                                }
                                FullAnswer::Select { value, .. } => {
                                    CompactAnswerRef::Selection(value.as_str())
                                }
                            };
                            (id.as_str(), answer)
                        })
                        .collect(),
                };
                serde_json::to_string(&compact)
            }
        }
    }

    /// Project any response to the smallest equivalent scalar answer map.
    pub fn compact(&self) -> CompactResponse {
        match self {
            Self::Compact(response) => response.clone(),
            Self::Full(response) => CompactResponse {
                answers: response
                    .answers
                    .iter()
                    .map(|(id, answer)| {
                        let answer = match answer {
                            FullAnswer::Boolean { value } | FullAnswer::Scale { value, .. } => {
                                CompactAnswer::Number(*value)
                            }
                            FullAnswer::Select { value, .. } => {
                                CompactAnswer::Selection(value.clone())
                            }
                        };
                        (id.clone(), answer)
                    })
                    .collect(),
            },
        }
    }
}

#[derive(Serialize)]
struct CompactResponseRef<'a> {
    answers: BTreeMap<&'a str, CompactAnswerRef<'a>>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum CompactAnswerRef<'a> {
    Number(f64),
    Selection(&'a str),
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{EvaluationOutput, FullAnswer, FullResponse};

    #[test]
    fn full_output_projects_to_compact_values() {
        let output = EvaluationOutput::Full(FullResponse {
            answers: BTreeMap::from([
                (
                    "route".to_owned(),
                    FullAnswer::Select {
                        value: "exact".to_owned(),
                        confidence: 0.9,
                        probabilities: BTreeMap::from([
                            ("exact".to_owned(), 0.95),
                            ("other".to_owned(), 0.05),
                        ]),
                    },
                ),
                (
                    "risk".to_owned(),
                    FullAnswer::Scale {
                        value: 1.5,
                        confidence: 0.8,
                        probabilities: vec![0.0, 0.5, 0.5],
                    },
                ),
            ]),
        });
        assert_eq!(
            serde_json::to_value(output.compact()).unwrap(),
            serde_json::json!({"answers": {"risk": 1.5, "route": "exact"}})
        );
    }
}
