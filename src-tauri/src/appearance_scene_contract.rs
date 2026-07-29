use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::{
    action::{ActionId, ActionParameters, ACTION_REGISTRY},
    backup::{BackupDraft, BackupEnvelope, BackupPayload, Fingerprint, ObservationBackup},
    journal::{JournalDatabase, PreparedItem, TransactionState},
    presentation::{parse_action_request, PreviewActionRequest},
};

const SCENES_JSON: &str = include_str!("../../src/appearance-scenes.json");
const ALLOWED_ACTION_IDS: [&str; 3] = [
    "theme.color_mode",
    "appearance.transparency",
    "appearance.window_color",
];
const FORBIDDEN_COPY: [&str; 3] = ["true black", "すべてのアプリ", "必ず"];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SceneContract {
    id: String,
    name: String,
    description: String,
    details: Vec<String>,
    swatch: SwatchContract,
    actions: Vec<SceneActionContract>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SwatchContract {
    surface: String,
    accent: String,
    translucent: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SceneActionContract {
    action_id: String,
    parameters: Map<String, Value>,
}

fn scenes() -> Vec<SceneContract> {
    serde_json::from_str(SCENES_JSON).expect("配色シーン定義を読める")
}

fn parsed_actions(scene: &SceneContract) -> Vec<ActionParameters> {
    scene
        .actions
        .iter()
        .map(|action| {
            parse_action_request(PreviewActionRequest {
                action_id: action.action_id.clone(),
                parameters: action.parameters.clone(),
            })
            .unwrap_or_else(|error| {
                panic!(
                    "{} の {} が通常のpreview引数として無効: {}",
                    scene.id, action.action_id, error.code
                )
            })
        })
        .collect()
}

#[test]
fn appearance_scenes_only_bundle_the_three_verified_actions() {
    let allowed = ALLOWED_ACTION_IDS.into_iter().collect::<BTreeSet<_>>();
    let catalog = scenes();
    assert!((3..=4).contains(&catalog.len()));

    for scene in &catalog {
        let actual = scene
            .actions
            .iter()
            .map(|action| action.action_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(actual, allowed, "{} のAction集合が契約外", scene.id);
        assert_eq!(
            scene.actions.len(),
            allowed.len(),
            "{} に重複Actionがある",
            scene.id
        );
    }
}

#[test]
fn appearance_scene_parameters_use_the_normal_preview_request_shapes() {
    for scene in scenes() {
        let parsed = parsed_actions(&scene);
        assert_eq!(parsed.len(), 3);
        for (request, parameters) in scene.actions.iter().zip(parsed) {
            assert_eq!(parameters.action_id().as_str(), request.action_id);
        }
    }
}

#[test]
fn appearance_scene_copy_stays_within_the_evidence() {
    for scene in scenes() {
        let text = std::iter::once(scene.name.as_str())
            .chain(std::iter::once(scene.description.as_str()))
            .chain(scene.details.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in FORBIDDEN_COPY {
            assert!(
                !text.to_lowercase().contains(forbidden),
                "{} に禁止表現 {forbidden:?} が含まれる",
                scene.id
            );
        }
        assert!(scene.swatch.surface.starts_with('#'));
        assert!(scene.swatch.accent.starts_with('#'));
        let _ = scene.swatch.translucent;
    }
}

#[test]
fn appearance_scene_requests_prepare_one_existing_journal_item_per_action() {
    for scene in scenes() {
        let journal = JournalDatabase::open_in_memory().expect("scene journal");
        let transaction_id = Uuid::new_v4();
        let items = parsed_actions(&scene)
            .into_iter()
            .enumerate()
            .map(|(ordinal, parameters)| {
                let item_id = Uuid::new_v4();
                let action_id = parameters.action_id();
                let fingerprint = Fingerprint::of_bytes(
                    format!("{}:{}", scene.id, action_id.as_str()).as_bytes(),
                );
                let backup = BackupEnvelope::from_draft(
                    BackupDraft {
                        precondition_fingerprint: fingerprint,
                        intended_fingerprint: fingerprint,
                        payload: BackupPayload::Observation(ObservationBackup {
                            source: "appearance scene journal contract".to_owned(),
                        }),
                    },
                    transaction_id,
                    item_id,
                    action_id,
                    ACTION_REGISTRY
                        .get(action_id)
                        .expect("scene action registered")
                        .metadata()
                        .action_version,
                    1,
                    26_200,
                );
                PreparedItem {
                    item_id,
                    ordinal: u32::try_from(ordinal).expect("three items fit u32"),
                    action_id,
                    action_version: ACTION_REGISTRY
                        .get(action_id)
                        .expect("scene action registered")
                        .metadata()
                        .action_version,
                    parameters,
                    resource_keys: vec![format!("appearance-scene:{}", action_id.as_str())],
                    backup,
                }
            })
            .collect::<Vec<_>>();

        journal
            .record_prepared_transaction(
                transaction_id,
                "manual preview",
                "manual",
                "appearance-scene-test",
                &items,
                1,
            )
            .expect("existing journal accepts scene items");

        for (apply_order, item) in items.iter().enumerate() {
            journal
                .mark_item_applying(
                    transaction_id,
                    item.item_id,
                    u32::try_from(apply_order).expect("three items fit u32"),
                    2,
                )
                .expect("mark scene item applying");
            journal
                .mark_item_applied(transaction_id, item.item_id, &item.backup, 3)
                .expect("mark scene item applied");
        }
        journal
            .set_transaction_state(transaction_id, TransactionState::Succeeded, true, 4)
            .expect("finish scene transaction");

        let timeline = journal.list_timeline(10).expect("read scene timeline");
        assert_eq!(timeline.len(), 3, "{} は1 Action 1項目", scene.id);
        assert!(
            timeline.iter().all(|item| item.status == "succeeded"),
            "{} の全項目が成功状態で残る",
            scene.id
        );
        let stored_ids = timeline
            .iter()
            .map(|item| item.action_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            stored_ids,
            ALLOWED_ACTION_IDS.into_iter().collect(),
            "{} のjournal項目がAction要求と一致",
            scene.id
        );
    }
}

#[test]
fn scene_action_ids_are_registered() {
    for action_id in ALLOWED_ACTION_IDS {
        let parsed = action_id.parse::<ActionId>().expect("known action id");
        assert!(ACTION_REGISTRY.get(parsed).is_some());
    }
}
