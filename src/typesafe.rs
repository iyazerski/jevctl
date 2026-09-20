use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::AppError;
use crate::protocol::{
    CompactAnswer, CompactResponse, EvaluationOutput, EvaluationRequest, FullAnswer, FullResponse,
    OutputDetail, Question,
};

const MODEL: &str = "jev-latest";

#[derive(Debug, Serialize)]
pub(crate) struct ApiRequest<'a> {
    state: Cow<'a, Value>,
    model: &'static str,
    questions: BTreeMap<&'a str, ApiQuestion<'a>>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ApiQuestion<'a> {
    Noul {
        instructions: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        criteria: Option<NoulCriteria<'a>>,
    },
    Choice {
        instructions: &'a str,
        criteria: &'a BTreeMap<String, String>,
    },
    Score {
        instructions: &'a str,
        criteria: &'a [String],
    },
}

#[derive(Debug, Serialize)]
struct NoulCriteria<'a> {
    #[serde(rename = "true", skip_serializing_if = "Option::is_none")]
    yes: Option<&'a str>,
    #[serde(rename = "false", skip_serializing_if = "Option::is_none")]
    no: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ApiResponse {
    answers: BTreeMap<String, ApiAnswer>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ApiAnswer {
    Noul {
        noul: f64,
    },
    Choice {
        choice: String,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
    Score {
        score: f64,
        legend: BTreeMap<String, String>,
        probabilities: BTreeMap<String, f64>,
        confidence: f64,
    },
}

impl<'a> ApiRequest<'a> {
    /// Translate a provider-independent evaluation into the current upstream request.
    pub(crate) fn from_public(request: &'a EvaluationRequest) -> Self {
        let questions = request
            .questions
            .iter()
            .map(|(id, question)| {
                let question = match question {
                    Question::Boolean {
                        prompt,
                        yes_when,
                        no_when,
                    } => {
                        let criteria = if yes_when.is_some() || no_when.is_some() {
                            Some(NoulCriteria {
                                yes: yes_when.as_deref(),
                                no: no_when.as_deref(),
                            })
                        } else {
                            None
                        };
                        ApiQuestion::Noul {
                            instructions: prompt,
                            criteria,
                        }
                    }
                    Question::Select { prompt, options } => ApiQuestion::Choice {
                        instructions: prompt,
                        criteria: options,
                    },
                    Question::Scale { prompt, levels } => ApiQuestion::Score {
                        instructions: prompt,
                        criteria: levels,
                    },
                };
                (id.as_str(), question)
            })
            .collect();
        let state = match request.context {
            Value::String(_) | Value::Array(_) | Value::Object(_) => {
                Cow::Borrowed(&request.context)
            }
            _ => Cow::Owned(serde_json::json!({"value": &request.context})),
        };
        Self {
            state,
            model: MODEL,
            questions,
        }
    }
}

impl ApiResponse {
    /// Validate and normalize an upstream result into the stable public response.
    pub(crate) fn normalize(
        self,
        request: &EvaluationRequest,
        detail: OutputDetail,
    ) -> Result<EvaluationOutput, AppError> {
        let mut answers = self.answers;
        if request.questions.len() != answers.len() || !request.questions.keys().eq(answers.keys())
        {
            return invalid("answer IDs do not match requested question IDs");
        }

        let mut compact = BTreeMap::new();
        let mut full = BTreeMap::new();
        for (id, question) in &request.questions {
            let (id, answer) = answers.remove_entry(id).ok_or_else(|| {
                AppError::InvalidResponse(format!("missing answer for question {id:?}"))
            })?;
            match (question, answer) {
                (Question::Boolean { .. }, ApiAnswer::Noul { noul }) => {
                    validate_probability(noul, &id)?;
                    match detail {
                        OutputDetail::Compact => {
                            compact.insert(id, CompactAnswer::Number(noul));
                        }
                        OutputDetail::Full => {
                            full.insert(id, FullAnswer::Boolean { value: noul });
                        }
                    }
                }
                (
                    Question::Select { options, .. },
                    ApiAnswer::Choice {
                        choice,
                        probabilities,
                        confidence,
                    },
                ) => {
                    validate_probability(confidence, &id)?;
                    validate_distribution(&probabilities, options, &id)?;
                    if !options.contains_key(&choice) {
                        return invalid(format!(
                            "question {id:?} selected unknown option {choice:?}"
                        ));
                    }
                    match detail {
                        OutputDetail::Compact => {
                            compact.insert(id, CompactAnswer::Selection(choice));
                        }
                        OutputDetail::Full => {
                            full.insert(
                                id,
                                FullAnswer::Select {
                                    value: choice,
                                    confidence,
                                    probabilities,
                                },
                            );
                        }
                    }
                }
                (
                    Question::Scale { levels, .. },
                    ApiAnswer::Score {
                        score,
                        legend,
                        probabilities,
                        confidence,
                    },
                ) => {
                    validate_probability(confidence, &id)?;
                    validate_score(score, levels.len(), &id)?;
                    validate_score_distribution(&probabilities, &legend, levels, &id)?;
                    match detail {
                        OutputDetail::Compact => {
                            compact.insert(id, CompactAnswer::Number(score));
                        }
                        OutputDetail::Full => {
                            let probs = (0..levels.len())
                                .map(|index| probabilities[&index.to_string()])
                                .collect();
                            full.insert(
                                id,
                                FullAnswer::Scale {
                                    value: score,
                                    confidence,
                                    probabilities: probs,
                                },
                            );
                        }
                    }
                }
                _ => return invalid(format!("question {id:?} received the wrong answer kind")),
            }
        }

        Ok(match detail {
            OutputDetail::Compact => {
                EvaluationOutput::Compact(CompactResponse { answers: compact })
            }
            OutputDetail::Full => EvaluationOutput::Full(FullResponse { answers: full }),
        })
    }
}

fn validate_probability(value: f64, id: &str) -> Result<(), AppError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return invalid(format!("question {id:?} returned an invalid probability"));
    }
    Ok(())
}

fn validate_distribution(
    probabilities: &BTreeMap<String, f64>,
    options: &BTreeMap<String, String>,
    id: &str,
) -> Result<(), AppError> {
    if probabilities.len() != options.len() || !probabilities.keys().eq(options.keys()) {
        return invalid(format!(
            "question {id:?} returned probabilities for unexpected options"
        ));
    }
    validate_probability_sum(probabilities.values().copied(), id)
}

fn validate_score_distribution(
    probabilities: &BTreeMap<String, f64>,
    legend: &BTreeMap<String, String>,
    levels: &[String],
    id: &str,
) -> Result<(), AppError> {
    if probabilities.len() != levels.len() || legend.len() != levels.len() {
        return invalid(format!(
            "question {id:?} returned an invalid scale distribution"
        ));
    }
    for (index, level) in levels.iter().enumerate() {
        let key = index.to_string();
        if legend.get(&key) != Some(level) {
            return invalid(format!(
                "question {id:?} returned a mismatched scale legend"
            ));
        }
        if !probabilities.contains_key(&key) {
            return invalid(format!(
                "question {id:?} returned an invalid scale distribution"
            ));
        }
    }
    validate_probability_sum(probabilities.values().copied(), id)
}

fn validate_probability_sum(values: impl Iterator<Item = f64>, id: &str) -> Result<(), AppError> {
    let mut sum = 0.0;
    for value in values {
        validate_probability(value, id)?;
        sum += value;
    }
    if (sum - 1.0).abs() > 0.01 {
        return invalid(format!("question {id:?} probabilities do not sum to one"));
    }
    Ok(())
}

fn validate_score(value: f64, level_count: usize, id: &str) -> Result<(), AppError> {
    let max = (level_count - 1) as f64;
    if !value.is_finite() || !(0.0..=max).contains(&value) {
        return invalid(format!("question {id:?} returned an invalid scale value"));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, AppError> {
    Err(AppError::InvalidResponse(message.into()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{ApiRequest, ApiResponse};
    use crate::protocol::{EvaluationRequest, OutputDetail, Question};

    fn request() -> EvaluationRequest {
        EvaluationRequest {
            context: json!({"text": "Ship the exact fix"}),
            questions: BTreeMap::from([
                (
                    "keep".to_owned(),
                    Question::Boolean {
                        prompt: "Keep it?".to_owned(),
                        yes_when: Some("Needed".to_owned()),
                        no_when: Some("Extra".to_owned()),
                    },
                ),
                (
                    "route".to_owned(),
                    Question::Select {
                        prompt: "Which route?".to_owned(),
                        options: BTreeMap::from([
                            ("exact".to_owned(), "Exact".to_owned()),
                            ("generic".to_owned(), "Generic".to_owned()),
                        ]),
                    },
                ),
                (
                    "risk".to_owned(),
                    Question::Scale {
                        prompt: "Risk?".to_owned(),
                        levels: vec!["Low".to_owned(), "Medium".to_owned(), "High".to_owned()],
                    },
                ),
            ]),
            detail: None,
        }
    }

    fn response_json() -> serde_json::Value {
        json!({
            "model": "hidden",
            "answers": {
                "keep": {"type": "noul", "noul": 0.9},
                "route": {
                    "type": "choice", "choice": "exact",
                    "probabilities": {"exact": 0.8, "generic": 0.2}, "confidence": 0.7
                },
                "risk": {
                    "type": "score", "score": 1.7,
                    "legend": {"0": "Low", "1": "Medium", "2": "High"},
                    "probabilities": {"0": 0.05, "1": 0.2, "2": 0.75}, "confidence": 0.8
                }
            },
            "usage": {"input_tokens": 1, "output_tokens": 1}
        })
    }

    fn response() -> ApiResponse {
        serde_json::from_value(response_json()).unwrap()
    }

    #[test]
    fn maps_public_request_to_upstream_contract() {
        let request = request();
        let value = serde_json::to_value(ApiRequest::from_public(&request)).unwrap();
        assert_eq!(value["model"], "jev-latest");
        assert_eq!(value["state"], json!({"text": "Ship the exact fix"}));
        assert_eq!(value["questions"]["keep"]["type"], "noul");
        assert_eq!(value["questions"]["route"]["type"], "choice");
        assert_eq!(value["questions"]["risk"]["type"], "score");
    }

    #[test]
    fn wraps_scalar_context_for_upstream_compatibility() {
        let mut request = request();
        request.context = json!(42);
        let value = serde_json::to_value(ApiRequest::from_public(&request)).unwrap();
        assert_eq!(value["state"], json!({"value": 42}));
    }

    #[test]
    fn compact_output_is_only_scalar_answers() {
        let value = serde_json::to_value(
            response()
                .normalize(&request(), OutputDetail::Compact)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            value,
            json!({"answers": {"keep": 0.9, "risk": 1.7, "route": "exact"}})
        );
        assert!(serde_json::to_vec(&value).unwrap().len() < 70);
    }

    #[test]
    fn full_output_omits_upstream_metadata() {
        let value = serde_json::to_value(
            response()
                .normalize(&request(), OutputDetail::Full)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value.as_object().unwrap().len(), 1);
        let encoded = serde_json::to_string(&value).unwrap();
        for hidden in ["model", "usage", "noul", "choice", "score", "legend"] {
            assert!(!encoded.contains(hidden));
        }
    }

    #[test]
    fn rejects_distribution_with_missing_option() {
        let mut value = response_json();
        value["answers"]["route"]["probabilities"] = json!({"exact": 1.0});
        let response: ApiResponse = serde_json::from_value(value).unwrap();
        let error = response
            .normalize(&request(), OutputDetail::Compact)
            .unwrap_err();
        assert!(error.to_string().contains("unexpected options"));
    }
}
