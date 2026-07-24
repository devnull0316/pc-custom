use std::collections::{BTreeMap, BTreeSet};

use crate::actions::{
    ACTIVE_SCHEME_CHECK_ACTION, CLOCK_SECONDS_ACTION, COLOR_MODE_ACTION, PREVENT_SLEEP_ACTION,
    PROCESS_WATCH_ACTION, SHOW_EXTENSIONS_ACTION, SHOW_HIDDEN_ACTION, TASK_VIEW_ACTION,
    TRANSPARENCY_ACTION, WIDGETS_ACTION,
};

use super::{Action, ActionError, ActionErrorCode, ActionId, ActionResult, ActionStage};

static REGISTERED_ACTIONS: [&'static dyn Action; 10] = [
    &PREVENT_SLEEP_ACTION,
    &ACTIVE_SCHEME_CHECK_ACTION,
    &SHOW_EXTENSIONS_ACTION,
    &SHOW_HIDDEN_ACTION,
    &CLOCK_SECONDS_ACTION,
    &TRANSPARENCY_ACTION,
    &TASK_VIEW_ACTION,
    &WIDGETS_ACTION,
    &COLOR_MODE_ACTION,
    &PROCESS_WATCH_ACTION,
];

pub static ACTION_REGISTRY: ActionRegistry = ActionRegistry {
    actions: &REGISTERED_ACTIONS,
};

#[derive(Clone, Copy)]
pub struct ActionRegistry {
    actions: &'static [&'static dyn Action],
}

impl ActionRegistry {
    pub fn get(&self, id: ActionId) -> Option<&'static dyn Action> {
        self.actions
            .iter()
            .copied()
            .find(|action| action.metadata().id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &'static dyn Action> + '_ {
        self.actions.iter().copied()
    }

    /// Run at startup. A failure means the mutation surface must remain in safe mode.
    pub fn validate(&self) -> ActionResult<()> {
        let mut ids = BTreeSet::new();
        for action in self.actions {
            let metadata = action.metadata();
            metadata.validate_static_contract().map_err(|detail| {
                ActionError::new(
                    ActionErrorCode::InternalInvariant,
                    ActionStage::Validate,
                    false,
                    "action.registry.invalid_metadata",
                )
                .with_safe_detail(detail)
            })?;
            if !ids.insert(metadata.id) {
                return Err(registry_error("duplicate Action ID"));
            }
        }
        if ids != ActionId::ALL.into_iter().collect() {
            return Err(registry_error("compile-time Action set is incomplete"));
        }

        for action in self.actions {
            let metadata = action.metadata();
            for referenced in metadata
                .dependencies
                .iter()
                .chain(metadata.conflicts.iter())
            {
                if !ids.contains(referenced) {
                    return Err(registry_error("Action references an unknown Action ID"));
                }
            }
        }
        self.validate_dependency_acyclic()?;
        Ok(())
    }

    fn validate_dependency_acyclic(&self) -> ActionResult<()> {
        let mut colors = BTreeMap::<ActionId, u8>::new();
        for id in ActionId::ALL {
            if colors.get(&id).copied().unwrap_or(0) == 0 {
                self.visit_dependency(id, &mut colors)?;
            }
        }
        Ok(())
    }

    fn visit_dependency(
        &self,
        id: ActionId,
        colors: &mut BTreeMap<ActionId, u8>,
    ) -> ActionResult<()> {
        match colors.get(&id).copied().unwrap_or(0) {
            1 => return Err(registry_error("Action dependency cycle detected")),
            2 => return Ok(()),
            _ => {}
        }
        colors.insert(id, 1);
        let action = self
            .get(id)
            .ok_or_else(|| registry_error("Action missing during dependency validation"))?;
        for dependency in action.metadata().dependencies {
            self.visit_dependency(*dependency, colors)?;
        }
        colors.insert(id, 2);
        Ok(())
    }
}

fn registry_error(detail: &'static str) -> ActionError {
    ActionError::new(
        ActionErrorCode::InternalInvariant,
        ActionStage::Validate,
        false,
        "action.registry.safe_mode_required",
    )
    .with_safe_detail(detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiled_registry_is_complete_and_valid() {
        ACTION_REGISTRY.validate().expect("registry must be valid");
        for id in ActionId::ALL {
            assert_eq!(ACTION_REGISTRY.get(id).unwrap().metadata().id, id);
        }
    }
}
