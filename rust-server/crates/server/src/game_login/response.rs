use super::*;

#[cfg(test)]
pub(crate) fn apply_response_effects(
    effects: ResponseEffects,
    pre_pushes: &mut Vec<Vec<u8>>,
    post_pushes: &mut Vec<Vec<u8>>,
    handler_error: &mut Option<GameError>,
) {
    let (pre, post, error) = effects.into_parts();
    let now = current_unix_seconds();
    pre_pushes.extend(pre.into_iter().map(|response| response.encode_push(now)));
    post_pushes.extend(post.into_iter().map(|response| response.encode_push(now)));
    if let Some(error) = error {
        *handler_error = Some(error);
    }
}

#[cfg(not(test))]
pub(crate) fn apply_response_effects(
    effects: ResponseEffects,
    pre_pushes: &mut Vec<Response>,
    post_pushes: &mut Vec<Response>,
    handler_error: &mut Option<GameError>,
) {
    let (pre, post, error) = effects.into_parts();
    pre_pushes.extend(pre);
    post_pushes.extend(post);
    if let Some(error) = error {
        *handler_error = Some(error);
    }
}

pub(crate) fn response_payload(method: &str, payload: Vec<u8>) -> Option<Response> {
    Some(Response::raw(method, payload))
}
pub(crate) fn handler_payload(result: HandlerResult, method: &str) -> Option<Response> {
    result.into_response(method)
}
