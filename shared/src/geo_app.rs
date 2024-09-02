mod geo_types;
pub mod view_types;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::LazyLock;

use chrono::prelude::*;
use compact_str::{format_compact, CompactString, ToCompactString};
use crux_core::{render::Render, App};
use crux_geolocation::{GeoInfo, GeoOptions, GeoResult, Geolocation};
use crux_kv::{error::KeyValueError, KeyValue};
use crux_time::{Time, TimeResponse};
use geo_types::rtree_point;
pub use geo_types::SavedPos;
use jord::spherical::Sphere;
use rstar::RTree;
use serde::{Deserialize, Serialize};
use view_types::ViewModel;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Event {
    // Geolocation
    /// Start geolocation services.
    StartGeolocation,
    /// Got a position update.
    #[serde(skip)]
    GeolocationUpdate(GeoResult<GeoInfo>),
    /// Stop geolocation services.
    StopGeolocation,

    // Saved Positions
    /// Load saved positions from persistant storage.
    LoadSavedPositions,
    /// Save the current position with a name.
    SaveCurrPos(CompactString),
    /// Set saved positions from persistant storage.
    #[serde(skip)]
    SetSavedPositions {
        res: Result<Option<Vec<u8>>, KeyValueError>,
        key: CompactString,
    },
    /// Delete a saved position by its name.
    DelSavedPos(CompactString),
    /// View the n nearest saved positions. To hide all, set this to 0.
    ViewNSavedPositions(usize),

    // Time
    /// Tell that `Model::curr_time` should be updated.
    #[serde(skip)]
    UpdateCurrTime,
    /// Set `Model::curr_time`.
    SetCurrTime(crux_time::Instant),

    // Miscellaneous
    /// A message which should be displayed to the user.
    #[serde(skip)]
    Msg(CompactString),
    #[serde(skip)]
    None,
}

/// The planet we want to navigate on.
pub const PLANET: Sphere = Sphere::EARTH;

static UPDATE_CURR_TIME_INTERVAL: LazyLock<crux_time::Duration> =
    LazyLock::new(|| crux_time::Duration::from_secs(1).unwrap());
const GEOLOCATION_OPTIONS: GeoOptions = GeoOptions {
    maximum_age: 0,
    timeout: Some(27000),
    enable_high_accuracy: true,
};

/// Key when saving saved positions in persistant storage.
const SAVED_POSITIONS_KEY: &str = "saved_positions";

#[derive(Default)]
pub struct Model {
    /// The most recently received position.
    curr_pos: Option<GeoResult<GeoInfo>>,

    // Saved Positions
    /// An r-tree with all saved positions.
    saved_positions: RTree<SavedPos>,
    /// Saved positions by their names. This should probably be maid better in some way, but
    /// `RTree` doesn't support any other indexing than positions at the moment.
    saved_positions_names: HashMap<CompactString, SavedPos>,
    /// Saved positions to view. Must exist in `self.saved_positions`.
    view_saved_positions: Vec<SavedPos>,

    /// A message that should be viewed to the user.
    msg: CompactString,

    /// A by timestamp sorted list of all watched positions.
    all_positions: BTreeMap<chrono::DateTime<Utc>, GeoInfo>,
    /// The current time minus at most `UPDATE_CURR_TIME_AFTER`. Only availlable after the first
    /// call to `Event::StartGeolocation`.
    curr_time: Option<DateTime<Utc>>,
}

#[cfg_attr(feature = "typegen", derive(crux_core::macros::Export))]
#[derive(crux_core::macros::Effect)]
pub struct Capabilities {
    render: Render<Event>,
    storage: KeyValue<Event>,
    time: Time<Event>,
    geolocation: Geolocation<Event>,
}

#[derive(Default)]
pub struct GeoApp;

impl App for GeoApp {
    type Event = Event;
    type Model = Model;
    type ViewModel = ViewModel;
    type Capabilities = Capabilities;

    #[allow(unused_variables)]
    fn update(&self, event: Self::Event, model: &mut Self::Model, caps: &Self::Capabilities) {
        match event {
            // Geolocation
            Event::StartGeolocation => {
                caps.geolocation
                    .watch_position(GEOLOCATION_OPTIONS, Event::GeolocationUpdate);
                self.update(Event::UpdateCurrTime, model, caps);
            }
            Event::StopGeolocation => caps.geolocation.clear_watch(),
            Event::GeolocationUpdate(geo_result) => {
                model.curr_pos = Some(geo_result.clone());
                if let Ok(geo_info) = geo_result {
                    model.all_positions.insert(geo_info.timestamp, geo_info);
                }
            }

            // Saved Positions
            Event::LoadSavedPositions => caps.storage.get(SAVED_POSITIONS_KEY.to_string(), |res| {
                Event::SetSavedPositions {
                    res,
                    key: SAVED_POSITIONS_KEY.to_compact_string(),
                }
            }),
            Event::SetSavedPositions { res, key } => match res {
                Ok(Some(bytes)) => match bincode::deserialize(bytes.as_slice()) {
                    Ok((rtree, names)) => {
                        model.saved_positions = rtree;
                        model.saved_positions_names = names;
                        model.msg = format_compact!(
                            "Successfully loaded {} saved positions.",
                            model.saved_positions_names.len(),
                        )
                    }
                    Err(e) => {
                        model.msg = format_compact!(
                            "Browser Error: Error while decoding saved_positions: {e}"
                        )
                    }
                },
                Ok(None) => model.msg = format_compact!("No saved positions found."),
                Err(e) => {
                    model.msg =
                        format_compact!("Internal Error: When retrieving saved_positions: {e}")
                }
            },
            Event::SaveCurrPos(name) => {
                if let Some(Ok(geo)) = &model.curr_pos {
                    if model.saved_positions_names.contains_key(&name) {
                        model.msg =
                            format_compact!("Error: There is already a position named {name}");
                    } else {
                        let pos = SavedPos::new(name.clone(), geo);
                        model.saved_positions.insert(pos.clone());
                        model.saved_positions_names.insert(name, pos);
                        self.save_saved_positions(model, caps);
                    }
                } else {
                    model.msg = "Error: The current position is not known.".into();
                }
            }
            Event::DelSavedPos(name) => {
                if let Some(pos) = model.saved_positions_names.remove(&name) {
                    model.saved_positions.remove(&pos);
                    if let Some(idx) = model.view_saved_positions.iter().position(|x| *x == pos) {
                        model.view_saved_positions.remove(idx);
                    }
                    self.save_saved_positions(model, caps);
                    model.msg = format_compact!("{name} has been removed.");
                } else {
                    model.msg = format_compact!("Error: Position {name} does not exist.");
                }
            }
            Event::ViewNSavedPositions(n) => {
                model.view_saved_positions = if let Some(Ok(curr_pos)) = &model.curr_pos {
                    model
                        .saved_positions
                        .nearest_neighbor_iter(&rtree_point(curr_pos.coords))
                        .cloned()
                        .take(n)
                        .collect::<Vec<_>>()
                } else {
                    model.saved_positions.iter().cloned().take(n).collect()
                };
            }

            Event::Msg(msg) => model.msg = msg,

            // Time
            Event::UpdateCurrTime => {
                caps.time.now(|x| {
                    let TimeResponse::Now(x) = x else {
                        unreachable!()
                    };
                    Event::SetCurrTime(x)
                });
                caps.time
                    .notify_after(*UPDATE_CURR_TIME_INTERVAL, |_| Event::UpdateCurrTime);
            }
            Event::SetCurrTime(time) => {
                model.curr_time = Some(time.try_into().unwrap());
            }

            Event::None => (),
        }
        caps.render.render();
    }

    fn view(&self, model: &Self::Model) -> Self::ViewModel {
        ViewModel::new(model)
    }
}

/// Methods regarding saved positions.
impl GeoApp {
    fn save_saved_positions(&self, model: &mut Model, caps: &Capabilities) {
        caps.storage.set(
            SAVED_POSITIONS_KEY.to_string(),
            bincode::serialize(&(&model.saved_positions, &model.saved_positions_names)).unwrap(),
            |res| {
                if let Err(e) = res {
                    Event::Msg(format_compact!(
                        "Internal Error: Failed to serialize saved_positions: {e}"
                    ))
                } else {
                    Event::None
                }
            },
        );
    }
}
