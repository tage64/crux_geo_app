mod geo_types;
pub mod view_types;
use std::collections::BTreeMap;
use std::sync::LazyLock;

use chrono::prelude::*;
use compact_str::{format_compact, CompactString, ToCompactString};
use crux_core::{render::Render, App};
use crux_geolocation::{GeoInfo, GeoOptions, GeoResult, Geolocation};
use crux_kv::{error::KeyValueError, KeyValue};
use crux_time::{Time, TimeResponse};
pub use geo_types::SavedPos;
use rstar::RTree;
use serde::{Deserialize, Serialize};
use view_types::ViewModel;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Event {
    /// Start geolocation services.
    StartGeolocation,
    /// Stop geolocation services.
    StopGeolocation,
    /// Load saved positions from persistant storage.
    LoadSavedPositions,
    /// Save the current position with a name.
    SaveCurrPos(CompactString),

    // Responses:
    /// Set saved positions from persistant storage.
    #[serde(skip)]
    SetSavedPositions {
        res: Result<Option<Vec<u8>>, KeyValueError>,
        key: CompactString,
    },
    /// Got a position update.
    #[serde(skip)]
    GeolocationUpdate(GeoResult<GeoInfo>),
    /// A message which should be displayed to the user.
    #[serde(skip)]
    Msg(CompactString),
    /// Tell that `Model::curr_time` should be updated.
    #[serde(skip)]
    UpdateCurrTime,
    /// Set `Model::curr_time`.
    SetCurrTime(crux_time::Instant),
}

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
    saved_positions: RTree<SavedPos>,
    curr_pos: Option<GeoResult<GeoInfo>>,
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
            Event::StartGeolocation => {
                caps.geolocation
                    .watch_position(GEOLOCATION_OPTIONS, Event::GeolocationUpdate);
                self.update(Event::UpdateCurrTime, model, caps);
            }
            Event::StopGeolocation => caps.geolocation.clear_watch(),
            Event::LoadSavedPositions => caps.storage.get(SAVED_POSITIONS_KEY.to_string(), |res| {
                Event::SetSavedPositions {
                    res,
                    key: SAVED_POSITIONS_KEY.to_compact_string(),
                }
            }),
            Event::SetSavedPositions { res, key } => match res {
                Ok(Some(bytes)) => match bincode::deserialize(bytes.as_slice()) {
                    Ok(x) => model.saved_positions = x,
                    Err(e) => {
                        model.msg = format_compact!(
                            "Browser Error: Error while decoding saved_positions: {e}"
                        )
                    }
                },
                Ok(None) => (),
                Err(e) => {
                    model.msg =
                        format_compact!("Internal Error: When retrieving saved_positions: {e}")
                }
            },
            Event::SaveCurrPos(name) => {
                if let Some(Ok(geo)) = &model.curr_pos {
                    model.saved_positions.insert(SavedPos::new(name, geo));
                    caps.storage.set(
                        SAVED_POSITIONS_KEY.to_string(),
                        bincode::serialize(&model.saved_positions).unwrap(),
                        |res| {
                            Event::Msg(if let Err(e) = res {
                                format_compact!(
                                    "Internal Error: Failed to serialize saved_positions: {e}"
                                )
                            } else {
                                "Position saved successfully!".to_compact_string()
                            })
                        },
                    );
                } else {
                    model.msg = "Error: The current position is not known.".into();
                }
            }
            Event::GeolocationUpdate(geo_result) => {
                model.curr_pos = Some(geo_result.clone());
                if let Ok(geo_info) = geo_result {
                    model.all_positions.insert(geo_info.timestamp, geo_info);
                }
            }
            Event::Msg(msg) => model.msg = msg,
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
        }
        caps.render.render();
    }

    fn view(&self, model: &Self::Model) -> Self::ViewModel {
        ViewModel::new(model)
    }
}
