mod geo_types;
pub mod view_types;
use compact_str::format_compact;
use compact_str::CompactString;
use crux_core::{render::Render, App};
use crux_geolocation::{GeoInfo, GeoOptions, GeoResult, Geolocation};
pub use geo_types::SavedPos;
use rstar::RTree;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use view_types::{ViewModel, ViewVolocity};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Event {
    /// Start geolocation services.
    StartGeolocation,
    /// Stop geolocation services.
    StopGeolocation,
    /// Save the current position with a name.
    SaveCurrPos(CompactString),

    // Responses:
    /// Got a position update.
    #[serde(skip)]
    GeolocationUpdate(GeoResult<GeoInfo>),
    /// Save an earlier requested position.
    #[serde(skip)]
    SavePos(CompactString, GeoResult<GeoInfo>),
}

const GEOLOCATION_OPTIONS: GeoOptions = GeoOptions {
    maximum_age: 0,
    timeout: Some(27000),
    enable_high_accuracy: true,
};

#[derive(Default)]
pub struct Model {
    saved_positions: RTree<SavedPos>,
    curr_pos: Option<GeoResult<GeoInfo>>,
}

#[cfg_attr(feature = "typegen", derive(crux_core::macros::Export))]
#[derive(crux_core::macros::Effect)]
pub struct Capabilities {
    render: Render<Event>,
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
            }
            Event::StopGeolocation => caps.geolocation.clear_watch(),
            Event::SaveCurrPos(name) => {
                if let Some(Ok(geo)) = &model.curr_pos {
                    model.saved_positions.insert(SavedPos::new(name, geo));
                } else {
                    caps.geolocation
                        .get_position(GEOLOCATION_OPTIONS, |x| Event::SavePos(name, x));
                }
            }
            Event::GeolocationUpdate(geo_result) => model.curr_pos = Some(geo_result),
            Event::SavePos(name, geo_result) => {
                if let Ok(geo) = &geo_result {
                    model.saved_positions.insert(SavedPos::new(name, &geo));
                }
                model.curr_pos = Some(geo_result);
            }
        }
        caps.render.render();
    }

    fn view(&self, model: &Self::Model) -> Self::ViewModel {
        let curr_pos = if let Some(Ok(geo)) = &model.curr_pos {
            Some(SavedPos::new("Current position".into(), geo))
        } else {
            None
        };
        let gps_status = match &model.curr_pos {
            None => "No GPS information".into(),
            Some(Err(e)) => format_compact!("GPS Error: {}", e),
            Some(Ok(GeoInfo {
                accuracy: Some(accuracy),
                ..
            })) => format_compact!("Accuracy: {} m", accuracy.as_metres()),
            Some(Ok(_)) => "No accuracy information availlable".into(),
        };
        let near_positions = if let Some(curr_pos) = &curr_pos {
            model
                .saved_positions
                .nearest_neighbor_iter(&curr_pos.rtree_point())
                .take(10)
                .map(|x| x.view())
                .collect::<SmallVec<[_; 10]>>()
        } else {
            SmallVec::new()
        };
        ViewModel {
            curr_pos: curr_pos.map(|x| x.view()),
            volocity: model
                .curr_pos
                .as_ref()
                .map(|x| x.as_ref().ok())
                .flatten()
                .map(|x| ViewVolocity::new(x))
                .flatten(),
            near_positions,
            gps_status,
        }
    }
}
