//! Some types to store information to be viewed by the UI.
//!
//! The general theme is that these types formats numbers and units to strings so that the UI don't
//! have to bother with that.
use chrono::Local;
use compact_str::{format_compact, CompactString, ToCompactString};
use crux_geolocation::GeoInfo;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use super::{Model, SavedPos};

/// Precition for latitude and longitude.
const COORD_PRECITION: usize = 5;
/// Precition for altitude, volocity and other things.
const PRECITION: usize = 1;

/// Information about a position.
///
/// The information fields are represented by strings, including the value and the unit but not the
/// name. E.g. "59.265358° North", but the name "Latitude: " is not included.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
pub struct ViewPos {
    /// The name of a saved position or some dedicated string for the current position.
    pub name: CompactString,
    /// The latitude in decimal degrees.
    pub latitude: CompactString,
    /// The longitude in decimal degrees.
    pub longitude: CompactString,
    /// The altitude in decimal degrees.
    pub altitude: Option<CompactString>,
    /// The time when the position was captured.
    pub timestamp: CompactString,
}

impl SavedPos {
    pub fn view(&self) -> ViewPos {
        let latitude = self.coords.latitude().as_degrees();
        let longitude = self.coords.longitude().as_degrees();
        let north_south = if latitude >= 0.0 { "North" } else { "South" };
        let east_west = if longitude >= 0.0 { "East" } else { "West" };
        ViewPos {
            name: self.name.clone(),
            latitude: format_compact!("{:.*}° {}", COORD_PRECITION, latitude, north_south),
            longitude: format_compact!("{:.*}° {}", COORD_PRECITION, longitude, east_west,),
            altitude: self
                .altitude
                .map(|x| format_compact!("{:.*} meters", PRECITION, x.as_metres())),
            timestamp: self
                .timestamp
                .with_timezone(&Local)
                .format("%a %b %e %T %Y")
                .to_compact_string(),
        }
    }
}

/// Information about speed and bearing.
///
/// The information fields are represented by strings, including the value and the unit but not the
/// name. E.g. "3.1 km/h", but the name "Speed: " is not included.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
pub struct ViewVolocity {
    pub volocity: CompactString,
    pub bearing: Option<CompactString>,
}

impl ViewVolocity {
    pub fn new(geo: &GeoInfo) -> Option<Self> {
        if let Some(volocity) = geo.volocity {
            Some(Self {
                volocity: format_compact!("{:.*} m/s", PRECITION, volocity.as_metres_per_second()),
                bearing: geo
                    .bearing
                    .map(|x| format_compact!("{}°", x.as_degrees().round())),
            })
        } else {
            None
        }
    }
}

/// The entire view model. This is everything sent to the UI.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
pub struct ViewModel {
    /// Current position.
    pub curr_pos: Option<ViewPos>,
    /// Current volocity.
    pub volocity: Option<ViewVolocity>,
    /// Information about the GPS status. May display an error, especially if current_pos is
    /// `None`. Otherwise it should display accuracy and such.
    pub gps_status: CompactString,
    /// Near saved positions.
    pub near_positions: SmallVec<[ViewPos; 10]>,
    /// A message that should be displayed to the user.
    pub msg: Option<CompactString>,
}

impl ViewModel {
    pub fn new(model: &Model) -> Self {
        let curr_pos = if let Some(Ok(geo)) = &model.curr_pos {
            Some(SavedPos::new("Current position".into(), geo))
        } else {
            None
        };
        let gps_status = match &model.curr_pos {
            None => "No GPS information".into(),
            Some(Err(e)) => format_compact!("GPS Error: {}", e),
            Some(Ok(GeoInfo {
                accuracy,
                altitude_accuracy,
                ..
            })) => {
                let mut text = CompactString::new("");
                if let Some(a) = accuracy {
                    text += &format_compact!("Accuracy: {:.*} m, ", PRECITION, a.as_metres());
                }
                if let Some(aa) = altitude_accuracy {
                    text +=
                        &format_compact!("Altitude accuracy: {:.*} m, ", PRECITION, aa.as_metres());
                }
                let positions_in_last_minute = if let Some(curr_time) = model.curr_time {
                    model
                        .all_positions
                        .range(curr_time - chrono::TimeDelta::minutes(1)..)
                        .count()
                } else {
                    0
                };
                text +=
                    &format_compact!("{} positions in the last minute.", positions_in_last_minute);
                text
            }
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
        Self {
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
            msg: if model.msg.is_empty() {
                None
            } else {
                Some(model.msg.clone())
            },
        }
    }
}
