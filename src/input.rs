use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use crate::error::AppError;
use crate::protocol::EvaluationRequest;

const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;

/// Read and decode one bounded evaluation request from a file or stdin.
pub fn read_request(path: &Path) -> Result<EvaluationRequest, AppError> {
    let reader: Box<dyn Read> = if path == Path::new("-") {
        Box::new(io::stdin().lock())
    } else {
        Box::new(File::open(path).map_err(|source| AppError::InputIo {
            path: path.display().to_string(),
            source,
        })?)
    };
    decode_bounded(reader, &path.display().to_string())
}

fn decode_bounded(reader: impl Read, source_name: &str) -> Result<EvaluationRequest, AppError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| AppError::InputIo {
            path: source_name.to_owned(),
            source,
        })?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(AppError::InvalidInput(format!(
            "input exceeds {MAX_INPUT_BYTES} bytes"
        )));
    }
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{MAX_INPUT_BYTES, decode_bounded};

    #[test]
    fn rejects_oversized_input() {
        let input = vec![b' '; MAX_INPUT_BYTES as usize + 1];
        let error = decode_bounded(Cursor::new(input), "test").unwrap_err();
        assert_eq!(error.to_string(), "input exceeds 16777216 bytes");
    }
}
