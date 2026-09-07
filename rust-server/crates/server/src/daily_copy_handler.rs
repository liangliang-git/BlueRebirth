use super::catalog::ChapterCatalog;
use super::common::error::GameError;
use super::common::response::{HandlerResult, Response, ResponseEffects};
use super::*;

pub(super) fn handle_typed(
    account: &mut blueoath_domain::AccountState,
    chapter_catalog: Option<&ChapterCatalog>,
    method: &str,
    request_args: &[u8],
    effects: &mut ResponseEffects,
) -> HandlerResult {
    if !matches!(method, "dailycopy.GetData" | "dailycopy.SelectEx") {
        return HandlerResult::Empty;
    }

    let now = current_unix_seconds();
    let reset_day = (u64::from(now) + 8 * 60 * 60) / 86_400;
    if account.daily_copy.reset_day != reset_day as u32 {
        account.daily_copy.reset_day = u32::try_from(reset_day).unwrap_or(u32::MAX);
        account.daily_copy.challenge_times.clear();
        account.daily_copy.select_ex.clear();
    }

    if method == "dailycopy.SelectEx" {
        let Ok(request) = DailyCopySelectExRequest::decode(request_args) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "daily copy select request is invalid",
            ));
        };
        let known_chapter = chapter_catalog
            .map(|catalog| {
                catalog
                    .daily_chapters
                    .iter()
                    .any(|(id, _)| *id == request.chapter_id)
            })
            .unwrap_or(request.chapter_id == 1);
        if !known_chapter {
            return HandlerResult::Error(GameError::NotFound("daily copy chapter"));
        }
        let Ok(chapter_id) = blueoath_domain::ChapterId::new(request.chapter_id as u64) else {
            return HandlerResult::Error(GameError::InvalidRequest(
                "daily copy chapter is invalid",
            ));
        };
        account
            .daily_copy
            .select_ex
            .insert(chapter_id, request.select_ex);
        account
            .daily_copy
            .challenge_times
            .entry(chapter_id)
            .or_default();
    }

    let fallback_catalog;
    let catalog = match chapter_catalog {
        Some(catalog) => catalog,
        None => {
            fallback_catalog = ChapterCatalog::fallback();
            &fallback_catalog
        }
    };
    let payload = DailyCopyCodec::encode_with_progress(
        &catalog.daily_chapters,
        &catalog.daily_groups,
        &daily_copy_progress_from_typed_account(account, now),
        &[],
        &[],
    );
    effects.push_post(Response::raw("dailycopy.UpdateDailyCopyData", payload));
    HandlerResult::PushOnly
}

#[cfg(test)]
mod tests {
    use super::*;
    use blueoath_domain::{NewAccountFactory, ProfileId};

    #[test]
    fn typed_daily_copy_select_persists_in_domain_state_and_pushes_snapshot() {
        let mut account =
            NewAccountFactory::create(ProfileId::new("daily-copy-typed").unwrap(), "Captain");
        let mut args = Vec::new();
        append_varint_field(&mut args, 1, 1);
        append_varint_field(&mut args, 2, 1);
        let mut effects = ResponseEffects::default();

        assert!(matches!(
            handle_typed(
                &mut account,
                None,
                "dailycopy.SelectEx",
                &args,
                &mut effects,
            ),
            HandlerResult::PushOnly
        ));
        let chapter = blueoath_domain::ChapterId::new(1).unwrap();
        assert_eq!(account.daily_copy.select_ex.get(&chapter), Some(&true));
        assert_eq!(account.daily_copy.challenge_times.get(&chapter), Some(&0));
        let (pre, post, error) = effects.into_parts();
        assert!(pre.is_empty());
        assert_eq!(post.len(), 1);
        assert!(error.is_none());
    }
}
