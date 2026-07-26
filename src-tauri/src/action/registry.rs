use std::collections::{BTreeMap, BTreeSet};

use crate::actions::*;

use super::{Action, ActionError, ActionErrorCode, ActionId, ActionResult, ActionStage};

static REGISTERED_ACTIONS: [&'static dyn Action; 64] = [
    &PREVENT_SLEEP_ACTION,
    &ACTIVE_SCHEME_CHECK_ACTION,
    &POWER_SCHEME_SWITCH_ACTION,
    &SHOW_EXTENSIONS_ACTION,
    &SHOW_HIDDEN_ACTION,
    &CLOCK_SECONDS_ACTION,
    &TRANSPARENCY_ACTION,
    &TASK_VIEW_ACTION,
    &WIDGETS_ACTION,
    &ITEM_CHECKBOXES_ACTION,
    &COMPACT_VIEW_ACTION,
    &COLOR_MODE_ACTION,
    &PROCESS_WATCH_ACTION,
    &GAME_READINESS_CHECK_ACTION,
    &TASKBAR_SEARCH_MODE_ACTION,
    &TASKBAR_ALIGNMENT_ACTION,
    &START_LAYOUT_ACTION,
    &START_RECOMMENDATIONS_ACTION,
    &EXPLORER_LAUNCH_TARGET_ACTION,
    &EXPLORER_RECENT_FILES_ACTION,
    &TASKBAR_BUTTON_GROUPING_ACTION,
    &TASKBAR_FLASHING_ACTION,
    &TASKBAR_SHARE_WINDOW_ACTION,
    &TASKBAR_SHOW_DESKTOP_ACTION,
    &SEARCH_RECENT_ON_HOVER_ACTION,
    &TASKBAR_MULTI_MONITOR_ACTION,
    &TASKBAR_MULTI_MONITOR_MODE_ACTION,
    &TASKBAR_SECONDARY_BUTTON_GROUPING_ACTION,
    &START_SHOW_ALL_PINS_ACTION,
    &START_RECENT_APPS_ACTION,
    &ACCENT_START_TASKBAR_ACTION,
    &ACCENT_TITLE_BARS_ACTION,
    &AUTO_ACCENT_ACTION,
    &GAME_MODE_ACTION,
    &CONTROLLER_GAME_BAR_ACTION,
    &AUTOPLAY_ACTION,
    &USB_ERRORS_ACTION,
    &WEAK_CHARGER_ACTION,
    &AUTOCORRECT_ACTION,
    &DOUBLE_SPACE_ACTION,
    &AUTO_SHIFT_ACTION,
    &VOICE_TYPING_KEY_ACTION,
    &MULTILINGUAL_SUGGESTIONS_ACTION,
    &EXPLORER_STATUS_BAR_ACTION,
    &EXPLORER_INFO_TIPS_ACTION,
    &EXPLORER_HIDE_EMPTY_DRIVES_ACTION,
    &EXPLORER_NAV_EXPAND_CURRENT_ACTION,
    &EXPLORER_NAV_SHOW_ALL_ACTION,
    &EXPLORER_SEPARATE_PROCESS_ACTION,
    &EXPLORER_ICONS_ONLY_ACTION,
    &EXPLORER_DRIVE_LETTERS_ACTION,
    &EXPLORER_PREVIEW_HANDLERS_ACTION,
    &EXPLORER_SHARING_WIZARD_ACTION,
    &EXPLORER_ALWAYS_SHOW_MENUS_ACTION,
    &TASKBAR_ANIMATIONS_ACTION,
    &TOAST_BANNERS_ACTION,
    &STARTUP_INVENTORY_ACTION,
    &FREE_SPACE_CHECK_ACTION,
    &TEMP_FILES_CHECK_ACTION,
    &ACCENT_COLOR_CHECK_ACTION,
    &WINDOW_COLOR_ACTION,
    &POWERTOYS_STATUS_ACTION,
    &LAUNCH_APPS_ACTION,
    &WINDOWS_UPDATE_STATUS_ACTION,
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
