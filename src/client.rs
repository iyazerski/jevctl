use std::time::Duration;

use reqwest::header::RETRY_AFTER;
use reqwest::{StatusCode, Url};

use crate::error::AppError;
use crate::protocol::{EvaluationOutput, EvaluationRequest, OutputDetail};
use crate::typesafe::{ApiRequest, ApiResponse};

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const MAX_ATTEMPTS: usize = 3;

#[derive(Clone)]
pub struct EvaluationClient {
    http: reqwest::Client,
    endpoint: Url,
    api_key: String,
    max_attempts: usize,
}

impl EvaluationClient {
    /// Build an online client from the user-managed process environment.
    pub fn from_env() -> Result<Self, AppError> {
        let api_key = std::env::var("TYPESAFE_API_KEY")
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or(AppError::MissingApiKey)?;
        Self::new(
            Url::parse(ENDPOINT).expect("fixed endpoint must be valid"),
            api_key,
        )
    }

    fn new(endpoint: Url, api_key: String) -> Result<Self, AppError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| AppError::ServiceUnavailable)?;
        Ok(Self {
            http,
            endpoint,
            api_key,
            max_attempts: MAX_ATTEMPTS,
        })
    }

    /// Evaluate a validated batch and return only the stable public response.
    pub async fn evaluate(
        &self,
        request: &EvaluationRequest,
        detail: OutputDetail,
    ) -> Result<EvaluationOutput, AppError> {
        let api_request = ApiRequest::from_public(request);
        for attempt in 1..=self.max_attempts {
            let response = self
                .http
                .post(self.endpoint.clone())
                .bearer_auth(&self.api_key)
                .json(&api_request)
                .send()
                .await
                .map_err(|_| AppError::ServiceUnavailable)?;
            let status = response.status();
            if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
                return Err(AppError::Authentication);
            }
            if status == StatusCode::TOO_MANY_REQUESTS || status.as_u16() == 529 {
                if attempt == self.max_attempts {
                    return Err(AppError::RetriesExhausted {
                        attempts: self.max_attempts,
                    });
                }
                tokio::time::sleep(retry_delay(&response, attempt)).await;
                continue;
            }
            if status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::BAD_REQUEST {
                return Err(AppError::RequestRejected(
                    safe_error_message(response).await,
                ));
            }
            if !status.is_success() {
                return Err(AppError::ServiceUnavailable);
            }
            let response = response
                .json::<ApiResponse>()
                .await
                .map_err(|_| AppError::InvalidResponse("malformed JSON".to_owned()))?;
            return response.normalize(request, detail);
        }
        Err(AppError::RetriesExhausted {
            attempts: self.max_attempts,
        })
    }

    #[cfg(test)]
    fn for_test(endpoint: Url, api_key: &str, max_attempts: usize) -> Self {
        let mut client = Self::new(endpoint, api_key.to_owned()).unwrap();
        client.max_attempts = max_attempts;
        client
    }
}

fn retry_delay(response: &reqwest::Response, attempt: usize) -> Duration {
    if let Some(seconds) = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        return Duration::from_secs(seconds.min(30));
    }
    let base_ms = 100_u64.saturating_mul(1_u64 << (attempt - 1).min(8));
    Duration::from_millis(base_ms + fastrand::u64(0..=50))
}

async fn safe_error_message(response: reqwest::Response) -> String {
    let Ok(value) = response.json::<serde_json::Value>().await else {
        return "invalid request".to_owned();
    };
    value
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .or_else(|| value.get("message").and_then(serde_json::Value::as_str))
        .unwrap_or("invalid request")
        .chars()
        .take(300)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Instant;

    use reqwest::Url;
    use serde_json::json;

    use super::EvaluationClient;
    use crate::protocol::{EvaluationRequest, OutputDetail, Question};

    type MockResponse = (
        &'static str,
        Vec<(&'static str, &'static str)>,
        &'static str,
    );

    fn request() -> EvaluationRequest {
        EvaluationRequest {
            context: json!({"text": "Relevant"}),
            questions: BTreeMap::from([(
                "relevant".to_owned(),
                Question::Boolean {
                    prompt: "Is it relevant?".to_owned(),
                    yes_when: None,
                    no_when: None,
                },
            )]),
        }
    }

    fn serve(responses: Vec<MockResponse>) -> (Url, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, headers, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buffer = vec![0_u8; 16 * 1024];
                let size = stream.read(&mut buffer).unwrap();
                requests.push(String::from_utf8_lossy(&buffer[..size]).into_owned());
                let headers = headers
                    .into_iter()
                    .map(|(name, value)| format!("{name}: {value}\r\n"))
                    .collect::<String>();
                let response = format!(
                    "HTTP/1.1 {status}\r\n{headers}content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
            requests
        });
        (
            Url::parse(&format!("http://{address}/v1/systemone")).unwrap(),
            handle,
        )
    }

    fn ok_body() -> &'static str {
        "{\"model\":\"hidden\",\"answers\":{\"relevant\":{\"type\":\"noul\",\"noul\":0.9}},\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}"
    }

    #[tokio::test]
    async fn sends_authorized_request_and_normalizes_response() {
        let (url, server) = serve(vec![(
            "200 OK",
            vec![("content-type", "application/json")],
            ok_body(),
        )]);
        let client = EvaluationClient::for_test(url, "secret", 1);
        let output = client
            .evaluate(&request(), OutputDetail::Compact)
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(output).unwrap(),
            json!({"answers": {"relevant": 0.9}})
        );
        let requests = server.join().unwrap();
        let wire = &requests[0];
        assert!(
            wire.to_ascii_lowercase()
                .contains("authorization: bearer secret")
        );
        assert!(wire.contains("\"model\":\"jev-latest\""));
        assert!(wire.contains("\"type\":\"noul\""));
    }

    #[tokio::test]
    async fn retries_overload_then_succeeds() {
        let (url, server) = serve(vec![
            ("529 Overloaded", vec![("retry-after", "0")], ""),
            (
                "200 OK",
                vec![("content-type", "application/json")],
                ok_body(),
            ),
        ]);
        let client = EvaluationClient::for_test(url, "secret", 2);
        client
            .evaluate(&request(), OutputDetail::Compact)
            .await
            .unwrap();
        assert_eq!(server.join().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn maps_authentication_failure() {
        let (url, server) = serve(vec![("401 Unauthorized", vec![], "")]);
        let client = EvaluationClient::for_test(url, "secret", 1);
        let error = client
            .evaluate(&request(), OutputDetail::Compact)
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "authentication rejected");
        server.join().unwrap();
    }

    #[tokio::test]
    #[ignore = "local performance measurement"]
    async fn measures_local_client_round_trip() {
        let responses = (0..200)
            .map(|_| {
                (
                    "200 OK",
                    vec![("content-type", "application/json")],
                    ok_body(),
                )
            })
            .collect();
        let (url, server) = serve(responses);
        let client = EvaluationClient::for_test(url, "secret", 1);
        let mut timings = Vec::new();
        for _ in 0..200 {
            let started = Instant::now();
            client
                .evaluate(&request(), OutputDetail::Compact)
                .await
                .unwrap();
            timings.push(started.elapsed());
        }
        timings.sort_unstable();
        let median = timings[timings.len() / 2];
        let p95 = timings[timings.len() * 95 / 100];
        eprintln!("local client median={median:?} p95={p95:?}");
        assert!(median.as_millis() < 10, "median was {median:?}");
        assert_eq!(server.join().unwrap().len(), 200);
    }
}
