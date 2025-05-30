mod geo_traits;
mod geo_types;
pub mod view_types;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use chrono::prelude::*;
use compact_str::{CompactString, ToCompactString, format_compact};
use crux_core::{
    App,
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
use lazy_reaction::{DerivedSignal, ReactiveGraph, ReadSignal, Source, WriteSignal};
use rstar::RTree;
use serde::{Deserialize, Serialize};
use view_types::ViewModel;

use crate::FileDownloadOperation;

type Command = crux_core::Command<Effect, Event>;

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

/// An event from the shell. Either a user interaction or some information that was requested by
/// the app.
///
/// Some events are never retrieved from the shell but created by the app itself, those are marked
/// with `#[serde(skip)]` and if you are porting this application to a new platform you don't need
/// to care about them.
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

/// All the possible side effects of the application.
///
/// If you port this application to a new platform, you need to implement these effects.
#[effect(typegen)]
pub enum Effect {
    Render(RenderOperation),
    Storage(KeyValueOperation),
    Time(TimeRequest),
    Geolocation(GeoOperation),
    FileDownload(FileDownloadOperation),
}

/// The state of the application.
#[derive(Default)]
struct InnerModel {
    rgraph: ReactiveGraph,

    /// The most recently received position.
    curr_pos: WriteSignal<Option<GeoResult<GeoInfo>>>,

    // Saved Positions
    /// An r-tree with all saved positions.
    saved_positions: WriteSignal<Arc<RTree<SavedPos>>>,

    /// Saved positions by their names. This should probably be maid better in some way, but
    /// `RTree` doesn't support any other indexing than positions at the moment.
    saved_positions_names: HashMap<CompactString, SavedPos>,
    /// The number of saved positions the UI at most want to show.
    view_n_saved_positions: WriteSignal<usize>,

    // Recorded Ways
    /// All positions since the app was started.
    all_positions: WriteSignal<Arc<Option<RecordedWay>>>,
    /// Saved ways and their names.
    recorded_ways: WriteSignal<Arc<HashMap<CompactString, RecordedWay>>>,
    /// The number of recorded ways the UI at most want to show.
    view_n_recorded_ways: WriteSignal<usize>,

    /// A message that should be viewed to the user.
    msg: WriteSignal<CompactString>,

    /// The current time minus at most `UPDATE_CURR_TIME_AFTER`. Only availlable after the first
    /// call to `Event::StartGeolocation`.
    curr_time: WriteSignal<Option<DateTime<Utc>>>,
}

pub struct Model {
    inner: InnerModel,
    view: Mutex<DerivedSignal<Arc<ViewModel>>>,
    saved_positions_subscriber: ReadSignal<Arc<RTree<SavedPos>>>,
    recorded_ways_subscriber: ReadSignal<Arc<HashMap<CompactString, RecordedWay>>>,
}

impl Default for Model {
    fn default() -> Self {
        let inner = InnerModel::default();
        Self {
            view: Mutex::new(ViewModel::make(&inner)),
            saved_positions_subscriber: inner.saved_positions.subscribe(),
            recorded_ways_subscriber: inner.recorded_ways.subscribe(),
            inner,
        }
    }
}

#[derive(Default)]
pub struct GeoApp;

impl App for GeoApp {
    type Event = Event;
    type Model = Model;
    type ViewModel = Arc<ViewModel>;
    type Effect = Effect;
    type Capabilities = (); // FIXME: Depricated and will be removed.

    #[allow(unused_variables)]
    fn update(
        &self,
        event: Self::Event,
        model: &mut Self::Model,
        _: &Self::Capabilities, // Deprecated argument
    ) -> Command {
        let mut action = update(&mut model.inner, event);

        action = action.then(render());

        // Check if things should be saved to persistant storage.
        if let Some(updated_saved_positions) = model.saved_positions_subscriber.get() {
            action = action.and(save_saved_positions(&updated_saved_positions, &model.inner));
        }
        if let Some(updated_recorded_ways) = model.recorded_ways_subscriber.get() {
            action = action.and(save_recorded_ways(&updated_recorded_ways));
        }

        action
    }

    fn view(&self, model: &Self::Model) -> Self::ViewModel {
        let mut view = model.view.lock().unwrap();
        view.get().unwrap_or_else(|| view.get_existing())
    }
}

fn update(model: &mut InnerModel, event: Event) -> Command {
    match event {
        // Geolocation
        Event::StartGeolocation => Geolocation::watch_position(GEOLOCATION_OPTIONS)
            .then_send(Event::GeolocationUpdate)
            .and(Command::event(Event::UpdateCurrTime)),
        Event::StopGeolocation => Geolocation::clear_watch().into(),
        Event::GeolocationUpdate(geo_result) => {
            model.curr_pos.set(Some(geo_result.clone()));
            if let Ok(geo_info) = geo_result {
                // Update `model.all_positions`.
                model.all_positions.update(|all_positions| {
                    if let Some(rec) = Arc::make_mut(all_positions) {
                        rec.add(&geo_info);
                    } else {
                        let mut rec = RecordedWay::new();
                        rec.add(&geo_info);
                        *all_positions = Arc::new(Some(rec));
                    }
                });
            }

            Command::done()
        }

        // Persistant Data
        Event::LoadPersistantData => Command::all([
            load_persistant_data(SAVED_POSITIONS_KEY),
            load_persistant_data(RECORDED_WAYS_KEY),
        ]),
        Event::SetData { res, key } => {
            if let Err(e) = set_data(model, res, key) {
                model.msg.set(e);
            }
            Command::done()
        }
        Event::DownloadData => {
            let json = serde_json::json!({
                SAVED_POSITIONS_KEY: (&**model.saved_positions.value(), &model.saved_positions_names),
                RECORDED_WAYS_KEY: &**model.recorded_ways.value(),
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
            if model.saved_positions_names.contains_key(&name) {
                // Error: A position with this name already exists.
                model.msg.set(format_compact!(
                    "Error: There is already a position named {name}"
                ));
            } else {
                let curr_pos = model.curr_pos.value();
                if let Some(Ok(ref geo)) = *curr_pos {
                    let pos = SavedPos::new(name.clone(), &geo);
                    drop(curr_pos);

                    // Insert the new position in `model.saved_positions`.
                    model
                        .saved_positions
                        .update(|positions| Arc::make_mut(positions).insert(pos.clone()));
                    model.saved_positions_names.insert(name, pos);
                } else {
                    model
                        .msg
                        .set("Error: The current position is not known.".into());
                }
            }

            Command::done()
        }
        Event::DelSavedPos(name) => {
            if let Some(pos) = model.saved_positions_names.remove(&name) {
                // Remove from `model.saved_positions`.
                model
                    .saved_positions
                    .update(|positions| Arc::make_mut(positions).remove(&pos));

                model.msg.set(format_compact!("{name} has been removed."));
            } else {
                model
                    .msg
                    .set(format_compact!("Error: Position {name} does not exist."));
            }
            Command::done()
        }
        Event::ViewNSavedPositions(n) => {
            model.view_n_saved_positions.set_if_changed(n);
            Command::done()
        }

        // Recorded Ways
        Event::SaveAllPositions(name) => {
            if let Some(all_positions) = &**model.all_positions.value() {
                model.recorded_ways.update_with(|recorded_ways| {
                    if recorded_ways.contains_key(&name) {
                        model
                            .msg
                            .set(format_compact!("Error: The name {name} is already in use."));
                        (false, ())
                    } else {
                        Arc::make_mut(recorded_ways).insert(name, all_positions.clone());

                        // The call to `save_recorded_ways()` will cause a deadlock as it will try
                        // to read recorded_ways while it is written to in this function.
                        // TODO: Fix this by making save_recorded_ways an derived signal/effect of
                        // `model.recorded_ways` instead of a function called explicitly.
                        // (true, save_recorded_ways(model))
                        (true, ())
                    }
                })
            } else {
                model
                    .msg
                    .set(format_compact!("Error: No positions recorded."));
            }
            Command::done()
        }
        Event::DelRecordedWay(name) => {
            model.recorded_ways.update_with(|recorded_ways| {
                let recorded_ways = Arc::make_mut(recorded_ways);
                if recorded_ways.remove(&name).is_some() {
                    model.msg.set(format_compact!("{name} has been removed."));

                    // The call to `save_recorded_ways()` will cause a deadlock as it will try
                    // to read recorded_ways while it is written to in this function.
                    // TODO: Fix this by making save_recorded_ways an derived signal/effect of
                    // `model.recorded_ways` instead of a function called explicitly.
                    // (true, save_recorded_ways(model))
                    (true, ())
                } else {
                    model
                        .msg
                        .set(format_compact!("Error: Way {name} does not exist."));
                    (false, ())
                }
            });
            Command::done()
        }
        Event::ViewNRecordedWays(n) => {
            model.view_n_recorded_ways.set_if_changed(n);
            Command::done()
        }

        Event::Msg(msg) => {
            model.msg.set_if_changed(msg);
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
            model.curr_time.set_if_changed(Some(time.into()));
            Command::done()
        }

        Event::None => Command::done(),
    }
}

/// Get data from persistant storage.
fn load_persistant_data(key: &'static str) -> Command {
    KeyValue::get(key).then_send(move |res| Event::SetData {
        res,
        key: key.to_compact_string(),
    })
}

/// Set data received from persistant storage.
fn set_data(
    model: &mut InnerModel,
    res: Result<Option<Vec<u8>>, KeyValueError>,
    key: CompactString,
) -> Result<(), CompactString> {
    match (res, key) {
        (Ok(Some(bytes)), key) if key == SAVED_POSITIONS_KEY => {
            let (rtree, names) = bincode::deserialize(bytes.as_slice()).map_err(|e| {
                format_compact!("Browser Error: Error while decoding saved_positions: {e}")
            })?;
            model.saved_positions.set(Arc::new(rtree));
            model.saved_positions_names = names;
        }
        (Ok(Some(bytes)), key) if key == RECORDED_WAYS_KEY => {
            let recorded_ways = bincode::deserialize(bytes.as_slice()).map_err(|e| {
                format_compact!("Browser Error: Error while decoding saved ways: {e}")
            })?;
            model.recorded_ways.set(Arc::new(recorded_ways));
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

fn save_saved_positions(saved_positions: &RTree<SavedPos>, model: &InnerModel) -> Command {
    KeyValue::set(
        SAVED_POSITIONS_KEY,
        bincode::serialize(&(saved_positions, &model.saved_positions_names)).unwrap(),
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

fn save_recorded_ways(recorded_ways: &HashMap<CompactString, RecordedWay>) -> Command {
    KeyValue::set(
        RECORDED_WAYS_KEY.to_string(),
        bincode::serialize(recorded_ways).unwrap(),
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
