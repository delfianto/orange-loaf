use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::encoders::EncoderKind;
use crate::encoders::tracks::{AudioAction, AudioTrackSelection};
use crate::encoders::x265::X265Settings;
use crate::filters::FilterSpec;
use crate::probe::AudioTrack;
use crate::state::AppState;

/// Which tracks a rule considers. Deliberately a subset of the prior art's
/// model (which also has Title-substring and explicit Track-index matching)
/// — Language and Channels cover the actual common case ("keep English and
/// Japanese, drop the rest" / "keep anything already stereo as-is"), and
/// dropping the rest keeps this genuinely simple rather than reproducing
/// every knob the research turned up.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchItem {
    All,
    Language,
    Channels,
}

/// Whether a rule applies to *every* track it matches, or only the first —
/// e.g. "convert all commentary-channel-count tracks to AAC" (All) vs.
/// "keep just the first English track, drop any others" (First). The prior
/// art also has a `Last` variant; omitted here as the less common case of
/// an already-reduced pair.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchType {
    All,
    First,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRule {
    pub match_item: MatchItem,
    /// Ignored when `match_item` is `All`. For `Language`, an ffprobe-style
    /// code compared case-insensitively ("eng"); for `Channels`, a plain
    /// integer channel count ("2", "6") parsed at apply-time.
    pub match_input: String,
    pub match_type: MatchType,
    pub action: AudioAction,
}

/// A named, reusable encoder+filter preset, with optional audio
/// auto-selection rules — the tagged-enum `settings` field is what makes
/// this straightforward (notes/plan.md §4.4/§4.10): no "which of ~25
/// `Option<XSettings>` fields matches this profile's encoder" lookup, the
/// way the prior art's `Profile` model needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub settings: EncoderKind,
    pub filters: FilterSpec,
    pub audio_rules: Vec<AudioRule>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Profiles {
    pub profiles: Vec<Profile>,
}

/// Three starting presets covering this app's fastest/most-compatible/
/// highest-quality-per-byte encoders. Fully editable/deletable like any
/// other profile (see the module-level scope decision) — unlike the prior
/// art, nothing here re-injects or reverts them on load.
fn built_in_profiles() -> Vec<Profile> {
    vec![
        Profile {
            id: "built-in-balanced-hevc".to_owned(),
            name: "Balanced HEVC".to_owned(),
            settings: EncoderKind::X265(X265Settings::default()),
            filters: FilterSpec::default(),
            audio_rules: Vec::new(),
        },
        Profile {
            id: "built-in-fast-av1".to_owned(),
            name: "Fast AV1".to_owned(),
            settings: EncoderKind::SvtAv1(crate::encoders::svt_av1::SvtAv1Settings {
                speed: 10,
                crf: 30.0,
                tune: None,
            }),
            filters: FilterSpec::default(),
            audio_rules: Vec::new(),
        },
        Profile {
            id: "built-in-apple-silicon-fast".to_owned(),
            name: "Apple Silicon Fast".to_owned(),
            settings: EncoderKind::HevcVideotoolbox(
                crate::encoders::videotoolbox::HevcVideoToolboxSettings { quality: 65.0 },
            ),
            filters: FilterSpec::default(),
            audio_rules: Vec::new(),
        },
    ]
}

impl Profiles {
    pub fn file_path() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("ORANGELOAF_PROFILES") {
            return Some(PathBuf::from(path));
        }
        directories::ProjectDirs::from("dev", "orangeloaf", "OrangeLoaf")
            .map(|dirs| dirs.data_dir().join("profiles.json"))
    }

    pub fn load() -> Self {
        Self::file_path().map_or_else(
            || Self {
                profiles: built_in_profiles(),
            },
            |path| Self::load_from(&path),
        )
    }

    fn load_from(path: &std::path::Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_else(|| Self {
                profiles: built_in_profiles(),
            })
    }

    /// # Errors
    ///
    /// Returns an error if the profiles file location can't be determined,
    /// or if writing fails.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::file_path().ok_or("could not determine the profiles file location")?;
        self.save_to(&path)
    }

    fn save_to(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let contents = serde_json::to_string_pretty(self).map_err(|err| err.to_string())?;
        // Unique temp name — see queue.rs's `save_to` for why a fixed one
        // isn't safe against concurrent writers to the same path.
        let tmp_path = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
        std::fs::write(&tmp_path, &contents).map_err(|err| err.to_string())?;
        std::fs::rename(&tmp_path, path).map_err(|err| err.to_string())
    }
}

fn rule_matches(rule: &AudioRule, track: &AudioTrack) -> bool {
    match rule.match_item {
        MatchItem::All => true,
        MatchItem::Language => track
            .language
            .as_deref()
            .is_some_and(|lang| lang.eq_ignore_ascii_case(rule.match_input.trim())),
        MatchItem::Channels => rule
            .match_input
            .trim()
            .parse::<u32>()
            .is_ok_and(|channels| channels == track.channels),
    }
}

fn default_selection(track: &AudioTrack) -> AudioTrackSelection {
    AudioTrackSelection {
        index: track.index,
        source_channels: track.channels,
        enabled: true,
        action: AudioAction::Copy,
    }
}

/// Evaluates a profile's audio rules against a probed track list, producing
/// the same [`AudioTrackSelection`] shape manual track editing does. Rules
/// are evaluated in order per track, and the *last* matching rule wins —
/// this lets a later, more specific rule override an earlier catch-all
/// (`All`) one, without needing a separate priority field. A `First`-type
/// rule stops matching further tracks once it's matched one, regardless of
/// how many other tracks would otherwise qualify.
///
/// No rules at all (a profile that doesn't touch audio) reproduces the same
/// default every fresh probe gets: every track enabled, copied as-is.
pub fn apply_audio_rules(rules: &[AudioRule], tracks: &[AudioTrack]) -> Vec<AudioTrackSelection> {
    if rules.is_empty() {
        return tracks.iter().map(default_selection).collect();
    }

    let mut first_already_matched = vec![false; rules.len()];
    tracks
        .iter()
        .map(|track| {
            let mut selection = AudioTrackSelection {
                enabled: false,
                ..default_selection(track)
            };
            for (i, rule) in rules.iter().enumerate() {
                if rule.match_type == MatchType::First && first_already_matched[i] {
                    continue;
                }
                if rule_matches(rule, track) {
                    selection.enabled = true;
                    selection.action = rule.action.clone();
                    first_already_matched[i] = true;
                }
            }
            selection
        })
        .collect()
}

// tauri::command's argument extraction only supports owned `State<'_, T>`,
// not `&State<'_, T>` — clippy's by-reference suggestion doesn't apply to
// any command in this file, for the same reason as commands.rs/queue.rs.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn list_profiles(state: State<'_, AppState>) -> Profiles {
    state
        .profiles
        .lock()
        .expect("profiles mutex poisoned")
        .clone()
}

/// Creates a new profile (`id` is `None`) or overwrites an existing one in
/// place (`id` is `Some`, e.g. renaming or re-saving the current Convert
/// settings over it) — either way, this is a real edit, not the prior
/// art's create-only model with default profiles silently reverting on
/// every launch (see this module's scope note).
///
/// # Errors
///
/// Returns an error if `id` is `Some` but no profile has that id, or if
/// persisting the updated profile list fails.
#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
#[tauri::command]
pub fn save_profile(
    state: State<'_, AppState>,
    id: Option<String>,
    name: String,
    settings: EncoderKind,
    filters: FilterSpec,
    audio_rules: Vec<AudioRule>,
) -> Result<Profiles, String> {
    let mut profiles = state.profiles.lock().expect("profiles mutex poisoned");
    match id {
        Some(id) => {
            let profile = profiles
                .profiles
                .iter_mut()
                .find(|p| p.id == id)
                .ok_or_else(|| format!("no such profile: {id}"))?;
            profile.name = name;
            profile.settings = settings;
            profile.filters = filters;
            profile.audio_rules = audio_rules;
        }
        None => profiles.profiles.push(Profile {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            settings,
            filters,
            audio_rules,
        }),
    }
    profiles.save()?;
    Ok(profiles.clone())
}

/// # Errors
///
/// Returns an error if no profile has `id`, or if persisting fails.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn duplicate_profile(state: State<'_, AppState>, id: &str) -> Result<Profiles, String> {
    let mut profiles = state.profiles.lock().expect("profiles mutex poisoned");
    let original = profiles
        .profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("no such profile: {id}"))?;
    let copy = Profile {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!("{} (Copy)", original.name),
        settings: original.settings.clone(),
        filters: original.filters.clone(),
        audio_rules: original.audio_rules.clone(),
    };
    profiles.profiles.push(copy);
    profiles.save()?;
    Ok(profiles.clone())
}

/// # Errors
///
/// Returns an error if persisting the updated profile list fails. Removing
/// an id that doesn't exist is a silent no-op, not an error — the caller's
/// goal ("this profile shouldn't be in the list") is already satisfied.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn delete_profile(state: State<'_, AppState>, id: &str) -> Result<Profiles, String> {
    let mut profiles = state.profiles.lock().expect("profiles mutex poisoned");
    profiles.profiles.retain(|p| p.id != id);
    profiles.save()?;
    Ok(profiles.clone())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedProfile {
    pub settings: EncoderKind,
    pub filters: FilterSpec,
    pub audio_tracks: Vec<AudioTrackSelection>,
}

/// # Errors
///
/// Returns an error if no profile has `id`.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
pub fn apply_profile(
    state: State<'_, AppState>,
    id: &str,
    source_audio_tracks: Vec<AudioTrack>,
) -> Result<AppliedProfile, String> {
    let profiles = state.profiles.lock().expect("profiles mutex poisoned");
    let profile = profiles
        .profiles
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("no such profile: {id}"))?;
    Ok(AppliedProfile {
        settings: profile.settings.clone(),
        filters: profile.filters.clone(),
        audio_tracks: apply_audio_rules(&profile.audio_rules, &source_audio_tracks),
    })
}

#[cfg(test)]
mod tests {
    use super::{AudioRule, MatchItem, MatchType, apply_audio_rules};
    use crate::encoders::tracks::AudioAction;
    use crate::probe::AudioTrack;

    /// Pins the wire representation of these two plain unit-variant enums —
    /// serde's default (no `#[serde(tag = ...)]` needed, since no variant
    /// carries data) is a bare lowercase string per variant, but that's
    /// worth confirming once here rather than assuming it on the TS side.
    #[test]
    fn match_item_and_match_type_serialize_as_bare_camel_case_strings() {
        assert_eq!(
            serde_json::to_value(MatchItem::All).unwrap(),
            serde_json::json!("all")
        );
        assert_eq!(
            serde_json::to_value(MatchItem::Language).unwrap(),
            serde_json::json!("language")
        );
        assert_eq!(
            serde_json::to_value(MatchItem::Channels).unwrap(),
            serde_json::json!("channels")
        );
        assert_eq!(
            serde_json::to_value(MatchType::All).unwrap(),
            serde_json::json!("all")
        );
        assert_eq!(
            serde_json::to_value(MatchType::First).unwrap(),
            serde_json::json!("first")
        );
    }

    fn track(index: u32, language: &str, channels: u32) -> AudioTrack {
        AudioTrack {
            index,
            codec_name: "aac".to_owned(),
            channels,
            language: Some(language.to_owned()),
            title: None,
            is_default: false,
        }
    }

    #[test]
    fn no_rules_enables_every_track_as_copy_the_same_as_a_fresh_probe() {
        let tracks = vec![track(0, "eng", 2), track(1, "jpn", 6)];
        let selections = apply_audio_rules(&[], &tracks);
        assert_eq!(selections.len(), 2);
        assert!(selections.iter().all(|s| s.enabled));
        assert!(
            selections
                .iter()
                .all(|s| matches!(s.action, AudioAction::Copy))
        );
    }

    #[test]
    fn language_rule_only_enables_matching_tracks() {
        let tracks = vec![track(0, "eng", 2), track(1, "fra", 2), track(2, "jpn", 6)];
        let rules = vec![AudioRule {
            match_item: MatchItem::Language,
            match_input: "eng".to_owned(),
            match_type: MatchType::All,
            action: AudioAction::Copy,
        }];
        let selections = apply_audio_rules(&rules, &tracks);
        assert!(selections[0].enabled);
        assert!(!selections[1].enabled);
        assert!(!selections[2].enabled);
    }

    #[test]
    fn language_match_is_case_insensitive() {
        let tracks = vec![track(0, "ENG", 2)];
        let rules = vec![AudioRule {
            match_item: MatchItem::Language,
            match_input: "eng".to_owned(),
            match_type: MatchType::All,
            action: AudioAction::Copy,
        }];
        assert!(apply_audio_rules(&rules, &tracks)[0].enabled);
    }

    #[test]
    fn channels_rule_matches_on_parsed_channel_count() {
        let tracks = vec![track(0, "eng", 2), track(1, "eng", 6)];
        let rules = vec![AudioRule {
            match_item: MatchItem::Channels,
            match_input: "6".to_owned(),
            match_type: MatchType::All,
            action: AudioAction::Copy,
        }];
        let selections = apply_audio_rules(&rules, &tracks);
        assert!(!selections[0].enabled);
        assert!(selections[1].enabled);
    }

    #[test]
    fn first_match_type_only_enables_the_first_matching_track() {
        let tracks = vec![track(0, "eng", 2), track(1, "eng", 6), track(2, "eng", 2)];
        let rules = vec![AudioRule {
            match_item: MatchItem::Language,
            match_input: "eng".to_owned(),
            match_type: MatchType::First,
            action: AudioAction::Copy,
        }];
        let selections = apply_audio_rules(&rules, &tracks);
        assert!(selections[0].enabled);
        assert!(!selections[1].enabled);
        assert!(!selections[2].enabled);
    }

    #[test]
    fn a_later_rule_overrides_an_earlier_catch_all_rule_for_the_same_track() {
        let tracks = vec![track(0, "eng", 2)];
        let rules = vec![
            AudioRule {
                match_item: MatchItem::All,
                match_input: String::new(),
                match_type: MatchType::All,
                action: AudioAction::Copy,
            },
            AudioRule {
                match_item: MatchItem::Language,
                match_input: "eng".to_owned(),
                match_type: MatchType::All,
                action: AudioAction::Convert {
                    codec: "aac".to_owned(),
                    quality: 4,
                    downmix: None,
                },
            },
        ];
        let selections = apply_audio_rules(&rules, &tracks);
        assert!(selections[0].enabled);
        assert!(matches!(
            &selections[0].action,
            AudioAction::Convert { codec, .. } if codec == "aac"
        ));
    }

    #[test]
    fn an_unparseable_channels_match_input_matches_nothing_rather_than_panicking() {
        let tracks = vec![track(0, "eng", 2)];
        let rules = vec![AudioRule {
            match_item: MatchItem::Channels,
            match_input: "not-a-number".to_owned(),
            match_type: MatchType::All,
            action: AudioAction::Copy,
        }];
        assert!(!apply_audio_rules(&rules, &tracks)[0].enabled);
    }
}
