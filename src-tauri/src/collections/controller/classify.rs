use crate::collections::model::CollectionInstallView;

use super::model::{CollectionPrompt, RoundState};

/// Reads the decision a round outcome needs. A FOMOD choice wins over a manual file, which wins over a waiting
/// download: the controller always asks about one member at a time and the same round can contain all three kinds.
pub fn classify(view: &CollectionInstallView) -> RoundState {
    let prompt = view
        .needs_choice
        .first()
        .map(|choice| CollectionPrompt::Choice {
            item: choice.key.clone(),
            name: choice.name.clone(),
            path: choice.choice.path.display().to_string(),
            module_name: choice.choice.module_name.clone(),
            steps: choice.choice.steps.clone(),
        })
        .or_else(|| {
            view.needs_user.first().and_then(|key| {
                let item = view.session.plan.item(key)?;
                Some(CollectionPrompt::Manual {
                    item: key.clone(),
                    name: item.name.clone(),
                    url: match &item.source {
                        crate::domain::collection::CollectionSource::Browse { url } => {
                            Some(url.clone())
                        }
                        _ => None,
                    },
                    instructions: item.instructions.clone(),
                })
            })
        });
    let finished = view.session.counts.pending == 0
        && view.needs_choice.is_empty()
        && view.needs_user.is_empty();
    RoundState { prompt, finished }
}
