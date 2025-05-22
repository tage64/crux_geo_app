mod geo_traits;
mod geo_types;
pub mod view_types;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use chrono::prelude::*;
use compact_str::{CompactString, ToCompactString, format_compact};
use crux_core::{
    App, Command,
    macros::effect,
    render::{RenderOperation, render},
};
use crux_geolocation::{GeoInfo, GeoOperation, GeoOptions, GeoResult, Geolocation};
use crux_kv::{KeyValueOperation, command::KeyValue, error::KeyValueError};
use crux_time::{
    TimeRequest,
    command::{Time, TimerOutcome},
};
use geo_types::{RecordedWay, SavedPos, rtree_point};
use jord::spherical::Sphere;
use rstar::RTree;
use serde::{Deserialize, Serialize};
use view_types::ViewModel;

use crate::FileDownloadOperation;

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

    // Persistant Data
    /// Load Persistant Data.
    LoadPersistantData,
    /// Set data from persistant storage.
    #[serde(skip)]
    SetData {
        res: Result<Option<Vec<u8>>, KeyValueError>,
        key: CompactString,
    },
    /// Download the data
    DownloadData,

    // Saved Positions
    /// Save the current position with a name.
    SaveCurrPos(CompactString),
    /// Delete a saved position by its name.
    DelSavedPos(CompactString),
    /// View the n nearest saved positions. To hide all, set this to 0.
    ViewNSavedPositions(usize),

    // Recorded Ways
    /// Save the way since the app started.
    SaveAllPositions(CompactString),
    /// Delete a recorded way.
    DelRecordedWay(CompactString),
    /// View n recorded ways.
    ViewNRecordedWays(usize),

    // Time
    /// Tell that `Model::curr_time` should be updated.
    #[serde(skip)]
    UpdateCurrTime,
    /// Set `Model::curr_time`.
    SetCurrTime(SystemTime),

    // Miscellaneous
    /// A message which should be displayed to the user.
    #[serde(skip)]
    Msg(CompactString),
    #[serde(skip)]
    None,
}

/// The planet we want to navigate on.
pub const PLANET: Sphere = Sphere::EARTH;

const UPDATE_CURR_TIME_INTERVAL: Duration = Duration::from_secs(1);
const GEOLOCATION_OPTIONS: GeoOptions = GeoOptions {
    maximum_age: 0,
    timeout: Some(27000),
    enable_high_accuracy: true,
};

/// Key when saving saved positions in persistant storage.
const SAVED_POSITIONS_KEY: &str = "saved_positions";
/// Key when saving ways.
const RECORDED_WAYS_KEY: &str = "recorded_ways";

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
    /// The number of saved positions the UI at most want to show.
    view_n_saved_positions: usize,
    /// Saved positions to view. Must exist in `self.saved_positions`.
    view_saved_positions: Vec<SavedPos>,

    // Recorded Ways
    /// All positions since the app was started.
    all_positions: Option<RecordedWay>,
    /// Saved ways and their names.
    recorded_ways: HashMap<CompactString, RecordedWay>,
    /// The number of recorded ways the UI at most want to show.
    view_n_recorded_ways: usize,
    /// Names of recorded ways to view.
    view_recorded_ways: Vec<CompactString>,

    /// A message that should be viewed to the user.
    msg: CompactString,

    /// The current time minus at most `UPDATE_CURR_TIME_AFTER`. Only availlable after the first
    /// call to `Event::StartGeolocation`.
    curr_time: Option<DateTime<Utc>>,
}

#[effect(typegen)]
pub enum Effect {
    Render(RenderOperation),
    Storage(KeyValueOperation),
    Time(TimeRequest),
    Geolocation(GeoOperation),
    FileDownload(FileDownloadOperation),
}

#[derive(Default)]
pub struct GeoApp;

impl App for GeoApp {
    type Event = Event;
    type Model = Model;
    type ViewModel = ViewModel;
    type Effect = Effect;
    type Capabilities = (); // FIXME: Depricated and will be removed.

    #[allow(unused_variables)]
    fn update(
        &self,
        event: Self::Event,
        model: &mut Self::Model,
        _: &Self::Capabilities, // Deprecated argument
    ) -> Command<Effect, Event> {
        let action = match event {
            // Geolocation
            Event::StartGeolocation => Geolocation::watch_position(GEOLOCATION_OPTIONS)
                .then_send(Event::GeolocationUpdate)
                .and(Command::event(Event::UpdateCurrTime)),
            Event::StopGeolocation => Geolocation::clear_watch().into(),
            Event::GeolocationUpdate(geo_result) => {
                model.curr_pos = Some(geo_result.clone());
                if let Ok(geo_info) = geo_result {
                    if let Some(rec) = &mut model.all_positions {
                        rec.add(&geo_info);
                    } else {
                        let mut rec = RecordedWay::new();
                        rec.add(&geo_info);
                        model.all_positions = Some(rec);
                    }
                }

                Command::done()
            }

            // Persistant Data
            Event::LoadPersistantData => Command::all([
                self.load_persistant_data(SAVED_POSITIONS_KEY),
                self.load_persistant_data(RECORDED_WAYS_KEY),
            ]),
            Event::SetData { res, key } => {
                if let Err(e) = self.set_data(model, res, key) {
                    model.msg = e;
                }
                Command::done()
            }
            Event::DownloadData => {
                let json = serde_json::json!({
                    SAVED_POSITIONS_KEY: (&model.saved_positions, &model.saved_positions_names),
                    RECORDED_WAYS_KEY: &model.recorded_ways,
                });
                Command::notify_shell(FileDownloadOperation {
                    content: serde_json::to_vec(&json).unwrap(),
                    file_name: Some("geosuper_data.json".into()),
                    mime_type: Some("application/json".into()),
                })
                .into()
            }

            // Saved Positions
            Event::SaveCurrPos(name) => {
                if let Some(Ok(geo)) = &model.curr_pos {
                    if model.saved_positions_names.contains_key(&name) {
                        model.msg =
                            format_compact!("Error: There is already a position named {name}");
                        Command::done()
                    } else {
                        let pos = SavedPos::new(name.clone(), geo);
                        model.saved_positions.insert(pos.clone());
                        model.saved_positions_names.insert(name, pos);
                        // Update `model.view_saved_positions`.
                        self.view_saved_positions(model);
                        self.save_saved_positions(model)
                    }
                } else {
                    model.msg = "Error: The current position is not known.".into();
                    Command::done()
                }
            }
            Event::DelSavedPos(name) => {
                if let Some(pos) = model.saved_positions_names.remove(&name) {
                    model.saved_positions.remove(&pos);
                    // Update `model.view_saved_positions`.
                    self.view_saved_positions(model);
                    model.msg = format_compact!("{name} has been removed.");
                    self.save_saved_positions(model)
                } else {
                    model.msg = format_compact!("Error: Position {name} does not exist.");
                    Command::done()
                }
            }
            Event::ViewNSavedPositions(n) => {
                model.view_n_saved_positions = n;
                self.view_saved_positions(model);
                Command::done()
            }

            // Recorded Ways
            Event::SaveAllPositions(name) => {
                if let Some(all_positions) = &model.all_positions {
                    if model.recorded_ways.contains_key(&name) {
                        model.msg = format_compact!("Error: The name {name} is already in use.");
                        Command::done()
                    } else {
                        model.recorded_ways.insert(name, all_positions.clone());
                        self.view_recorded_ways(model);
                        self.save_recorded_ways(model)
                    }
                } else {
                    model.msg = format_compact!("Error: No positions recorded.");
                    Command::done()
                }
            }
            Event::DelRecordedWay(name) => {
                if let Some(way) = model.recorded_ways.remove(&name) {
                    // Update `model.view_recorded_ways`.
                    self.view_recorded_ways(model);
                    model.msg = format_compact!("{name} has been removed.");
                    self.save_recorded_ways(model)
                } else {
                    model.msg = format_compact!("Error: Way {name} does not exist.");
                    Command::done()
                }
            }
            Event::ViewNRecordedWays(n) => {
                model.view_n_recorded_ways = n;
                self.view_recorded_ways(model);
                Command::done()
            }

            Event::Msg(msg) => {
                model.msg = msg;
                Command::done()
            }

            // Time
            Event::UpdateCurrTime => Time::now().then_send(Event::SetCurrTime).then(
                Time::notify_after(UPDATE_CURR_TIME_INTERVAL)
                    .0
                    .then_send(|x| match x {
                        TimerOutcome::Completed(_) => Event::UpdateCurrTime,
                        TimerOutcome::Cleared => unreachable!(),
                    }),
            ),
            Event::SetCurrTime(time) => {
                model.curr_time = Some(time.into());
                Command::done()
            }

            Event::None => Command::done(),
        };

        action.then(render())
    }

    fn view(&self, model: &Self::Model) -> Self::ViewModel {
        ViewModel::new(model)
    }
}

impl GeoApp {
    /// Get data from persistant storage.
    fn load_persistant_data(&self, key: &'static str) -> Command<Effect, Event> {
        KeyValue::get(key).then_send(move |res| Event::SetData {
            res,
            key: key.to_compact_string(),
        })
    }

    /// Set data received from persistant storage.
    fn set_data(
        &self,
        model: &mut Model,
        res: Result<Option<Vec<u8>>, KeyValueError>,
        key: CompactString,
    ) -> Result<(), CompactString> {
        match (res, key) {
            (Ok(Some(bytes)), key) if key == SAVED_POSITIONS_KEY => {
                let (rtree, names) = bincode::deserialize(bytes.as_slice()).map_err(|e| {
                    format_compact!("Browser Error: Error while decoding saved_positions: {e}")
                })?;
                model.saved_positions = rtree;
                model.saved_positions_names = names;
                // Update `model.view_saved_positions`.
                self.view_saved_positions(model);
            }
            (Ok(Some(bytes)), key) if key == RECORDED_WAYS_KEY => {
                let recorded_ways = bincode::deserialize(bytes.as_slice()).map_err(|e| {
                    format_compact!("Browser Error: Error while decoding saved ways: {e}")
                })?;
                model.recorded_ways = recorded_ways;
                // Update `model.view_recorded_ways`.
                self.view_recorded_ways(model);
            }
            (Ok(Some(_)), key) => panic!("Bad key: {key}"),
            (Ok(None), _) => (),
            (Err(e), key) => {
                return Err(format_compact!(
                    "Internal Error: When retrieving {key}: {e}"
                ));
            }
        }
        Ok(())
    }

    fn save_saved_positions(&self, model: &mut Model) -> Command<Effect, Event> {
        KeyValue::set(
            SAVED_POSITIONS_KEY,
            bincode::serialize(&(&model.saved_positions, &model.saved_positions_names)).unwrap(),
        )
        .then_send(|res| {
            if let Err(e) = res {
                Event::Msg(format_compact!(
                    "Internal Error: Failed to serialize saved_positions: {e}"
                ))
            } else {
                Event::None
            }
        })
    }

    fn save_recorded_ways(&self, model: &mut Model) -> Command<Effect, Event> {
        KeyValue::set(
            RECORDED_WAYS_KEY.to_string(),
            bincode::serialize(&model.recorded_ways).unwrap(),
        )
        .then_send(|res| {
            if let Err(e) = res {
                Event::Msg(format_compact!(
                    "Internal Error: Failed to serialize recorded_ways: {e}"
                ))
            } else {
                Event::None
            }
        })
    }

    /// Select the saved positions to view.
    fn view_saved_positions(&self, model: &mut Model) {
        model.view_saved_positions = if let Some(Ok(curr_pos)) = &model.curr_pos {
            model
                .saved_positions
                .nearest_neighbor_iter(&rtree_point(&curr_pos.coords))
                .cloned()
                .take(model.view_n_saved_positions)
                .collect::<Vec<_>>()
        } else {
            model
                .saved_positions
                .iter()
                .cloned()
                .take(model.view_n_saved_positions)
                .collect()
        };
    }

    /// Select recorded ways to show.
    fn view_recorded_ways(&self, model: &mut Model) {
        // TODO: Use an algorithm to select the n nearest ways.
        model.view_recorded_ways = model
            .recorded_ways
            .keys()
            .cloned()
            .take(model.view_n_recorded_ways)
            .collect();
        model.view_recorded_ways.sort();
    }
}
