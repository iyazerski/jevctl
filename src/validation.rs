use crate::error::AppError;
use crate::protocol::{EvaluationRequest, Question};

/// Validate one public request before it can consume network time or tokens.
pub fn validate_request(request: &EvaluationRequest) -> Result<(), AppError> {
    if request.questions.is_empty() {
        return invalid("questions must not be empty");
    }

    for (id, question) in &request.questions {
        if id.trim().is_empty() {
            return invalid("question IDs must not be empty");
        }
        if question.prompt().trim().is_empty() {
            return invalid(format!("question {id:?} must have a non-empty prompt"));
        }

        match question {
            Question::Boolean {
                yes_when, no_when, ..
            } => {
                validate_optional_text(id, "yes_when", yes_when.as_deref())?;
                validate_optional_text(id, "no_when", no_when.as_deref())?;
            }
            Question::Select { options, .. } => {
                if !(2..=255).contains(&options.len()) {
                    return invalid(format!(
                        "question {id:?} must define between 2 and 255 options"
                    ));
                }
                for (option, description) in options {
                    if option.trim().is_empty() || description.trim().is_empty() {
                        return invalid(format!(
                            "question {id:?} option IDs and descriptions must not be empty"
                        ));
                    }
                }
            }
            Question::Scale { levels, .. } => {
                if !(2..=10).contains(&levels.len()) {
                    return invalid(format!(
                        "question {id:?} must define between 2 and 10 levels"
                    ));
                }
                if levels.iter().any(|level| level.trim().is_empty()) {
                    return invalid(format!("question {id:?} levels must not be empty"));
                }
            }
        }
    }
    Ok(())
}

fn validate_optional_text(id: &str, field: &str, value: Option<&str>) -> Result<(), AppError> {
    if value.is_some_and(|text| text.trim().is_empty()) {
        return invalid(format!("question {id:?} field {field:?} must not be empty"));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> Result<T, AppError> {
    Err(AppError::InvalidInput(message.into()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::validate_request;
    use crate::protocol::{EvaluationRequest, Question};

    fn request(question: Question) -> EvaluationRequest {
        EvaluationRequest {
            context: json!({"text": "example"}),
            questions: BTreeMap::from([("q".to_owned(), question)]),
            detail: None,
        }
    }

    #[test]
    fn accepts_each_question_kind() {
        let questions = [
            Question::Boolean {
                prompt: "Is it relevant?".to_owned(),
                yes_when: Some("Relevant".to_owned()),
                no_when: Some("Unrelated".to_owned()),
            },
            Question::Select {
                prompt: "Which route?".to_owned(),
                options: BTreeMap::from([
                    ("a".to_owned(), "First".to_owned()),
                    ("b".to_owned(), "Second".to_owned()),
                ]),
            },
            Question::Scale {
                prompt: "How risky?".to_owned(),
                levels: vec!["Low".to_owned(), "High".to_owned()],
            },
        ];
        for question in questions {
            validate_request(&request(question)).unwrap();
        }
    }

    #[test]
    fn rejects_empty_questions() {
        let request = EvaluationRequest {
            context: json!(null),
            questions: BTreeMap::new(),
            detail: None,
        };
        assert_eq!(
            validate_request(&request).unwrap_err().to_string(),
            "questions must not be empty"
        );
    }

    #[test]
    fn rejects_invalid_select_size() {
        let error = validate_request(&request(Question::Select {
            prompt: "Choose".to_owned(),
            options: BTreeMap::from([("only".to_owned(), "Only".to_owned())]),
        }))
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "question \"q\" must define between 2 and 255 options"
        );
    }

    #[test]
    fn rejects_invalid_scale_size() {
        let error = validate_request(&request(Question::Scale {
            prompt: "Rate".to_owned(),
            levels: vec!["Only".to_owned()],
        }))
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "question \"q\" must define between 2 and 10 levels"
        );
    }
}
