use super::*;

#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    message_type: String,
    #[serde(rename = "requestId")]
    request_id: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Serialize)]
struct Success<'a> {
    ok: bool,
    #[serde(rename = "requestId")]
    request_id: &'a str,
    #[serde(rename = "type")]
    message_type: &'a str,
    payload: Value,
}

#[derive(Debug, Serialize)]
struct Failure<'a> {
    ok: bool,
    #[serde(rename = "requestId")]
    request_id: &'a str,
    #[serde(rename = "type")]
    message_type: &'static str,
    error: String,
}

/// Processes one framed request and writes one framed response.
pub async fn process_frame<S>(stream: &mut S, state: &mut ServerState) -> Result<bool, ServerError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let Some(bytes) = FrameCodec::read(stream).await? else {
        return Ok(false);
    };
    let response = response_for_bytes(state, &bytes)?;
    FrameCodec::write(stream, &response).await?;
    Ok(true)
}

pub(super) fn response_for_bytes(
    state: &mut ServerState,
    bytes: &[u8],
) -> Result<Vec<u8>, ServerError> {
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    match dispatch(state, &envelope.message_type, envelope.payload) {
        Ok(payload) => Ok(serde_json::to_vec(&Success {
            ok: true,
            request_id: &envelope.request_id,
            message_type: &envelope.message_type,
            payload,
        })?),
        Err(error) => Ok(serde_json::to_vec(&Failure {
            ok: false,
            request_id: &envelope.request_id,
            message_type: "error",
            error: error.to_string(),
        })?),
    }
}

pub(super) fn prepare_local_request(
    state: &ServerState,
    bytes: &[u8],
) -> Result<(Vec<u8>, ServerState), ServerError> {
    let mut candidate = state.clone();
    let response = response_for_bytes(&mut candidate, bytes)?;
    Ok((response, candidate))
}

pub(super) fn storage_failure_response(
    bytes: &[u8],
    error: &ServerError,
) -> Result<Vec<u8>, ServerError> {
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    Ok(serde_json::to_vec(&Failure {
        ok: false,
        request_id: &envelope.request_id,
        message_type: "error",
        error: error.to_string(),
    })?)
}
