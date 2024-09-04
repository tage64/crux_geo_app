//! Some types to store information to be viewed by the UI.
//!
//! The general theme is that these types formats numbers and units to strings so that the UI don't
//! have to bother with that.

use arrayvec::ArrayVec;
use chrono::{prelude::*, TimeDelta};
use compact_str::{format_compact, CompactString, ToCompactString};
use crux_geolocation::GeoInfo;
use jord::{spherical::Sphere, LatLong};
use serde::{Deserialize, Serialize};

use super::{Model, Position, RecordedWay, SavedPos, PLANET};

/// Precition for latitude and longitude.
const COORD_PRECITION: usize = 5;
/// Precition for altitude, volocity and other things.
const PRECITION: usize = 1;

/// Format latitude, longitude, altitude and accuracy.
fn format_pos(pos: &Position) -> ArrayVec<CompactString, 5> {
    let latitude = pos.coords.latitude().as_degrees();
    let longitude = pos.coords.longitude().as_degrees();
    let north_south = if latitude >= 0.0 { "North" } else { "South" };
    let east_west = if longitude >= 0.0 { "East" } else { "West" };
    let mut properties = ArrayVec::new();
    properties.push(format_compact!(
        "Latitude: {:.*}° {}",
        COORD_PRECITION,
        latitude,
        north_south
    ));
    properties.push(format_compact!(
        "Longitude: {:.*}° {}",
        COORD_PRECITION,
        longitude,
        east_west,
    ));
    if let Some(altitude) = pos.altitude {
        properties.push(format_compact!(
            "Altitude: {:.*} meters",
            PRECITION,
            altitude.as_metres()
        ));
    }
    if let Some(accuracy) = pos.accuracy {
        properties.push(format_compact!(
            "Accuracy: {} meters",
            accuracy.as_metres().round()
        ));
    }
    if let Some(altitude_accuracy) = pos.altitude_accuracy {
        properties.push(format_compact!(
            "Altitude accuracy: {} meters",
            altitude_accuracy.as_metres().round()
        ));
    }
    properties
}

/// Format a timestamp.
fn format_timestamp(timestamp: DateTime<Utc>) -> CompactString {
    timestamp
        .with_timezone(&Local)
        .format("%a %b %e %T %Y")
        .to_compact_string()
}

/// Information about a saved position.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
pub struct ViewSavedPos {
    /// The name and (if it does exist) a distance and direction.
    pub summary: CompactString,
    /// A number of properties, like latitude and timestamp.
    pub properties: ArrayVec<CompactString, 6>,
}

impl ViewSavedPos {
    fn new(saved_pos: SavedPos, curr_pos: Option<LatLong>) -> Self {
        let summary = if let Some(curr_coords) = curr_pos {
            format_compact!(
                "{}: {} m, {}°",
                saved_pos.name,
                PLANET
                    .distance(curr_coords.to_nvector(), saved_pos.pos.coords.to_nvector())
                    .as_metres()
                    .round(),
                Sphere::initial_bearing(
                    curr_coords.to_nvector(),
                    saved_pos.pos.coords.to_nvector()
                )
                .as_degrees()
                .round()
            )
        } else {
            saved_pos.name
        };

        let mut properties = ArrayVec::new();
        properties.extend(format_pos(&saved_pos.pos));
        properties.push(format_compact!(
            "Saved at: {}",
            format_timestamp(saved_pos.timestamp)
        ));
        Self {
            summary,
            properties,
        }
    }
}

/// Information about speed and bearing.
fn format_speed_and_heading(geo: &GeoInfo) -> ArrayVec<CompactString, 2> {
    let mut properties = ArrayVec::new();
    if let Some(speed) = geo.volocity {
        properties.push(format_compact!(
            "Speed: {:.*} m/s",
            PRECITION,
            speed.as_metres_per_second()
        ));
    }
    if let Some(heading) = geo.bearing {
        properties.push(format_compact!("Heading {}°", heading.as_degrees().round()));
    }
    properties
}

/// Information about a way which is being recorded.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
pub struct ViewRecordedWay {
    /// The elapsed time, distance and average speed.
    pub summary: CompactString,
    /// A number of properties, like number of nodes.
    pub properties: ArrayVec<CompactString, 1>,
}

impl ViewRecordedWay {
    pub(crate) fn new(_rec: &RecordedWay) -> Self {
        todo!()
    }
}

/// The entire view model. This is everything sent to the UI.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
pub struct ViewModel {
    /// Information about the GPS status. May display an error, especially if current_pos is
    /// `None`. Otherwise it should display accuracy and such.
    pub gps_status: CompactString,
    /// Properties like latitude and volocity about the current position. May be empty.
    pub curr_pos_properties: ArrayVec<CompactString, 7>,
    /// Saved positions to show.
    pub saved_positions: Vec<ViewSavedPos>,
    /// A message that should be displayed to the user.
    pub msg: Option<CompactString>,
}

impl ViewModel {
    pub fn new(model: &Model) -> Self {
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
                let positions_in_last_minute = model
                    .all_positions
                    .as_ref()
                    .and_then(|rec| {
                        model
                            .curr_time
                            .as_ref()
                            .map(|t| rec.get_since(*t - TimeDelta::minutes(1)).0.len())
                    })
                    .unwrap_or(0);
                text +=
                    &format_compact!("{} positions in the last minute.", positions_in_last_minute);
                text
            }
        };
        let curr_pos: Option<&GeoInfo> = model.curr_pos.as_ref().map(|x| x.as_ref().ok()).flatten();
        let mut curr_pos_properties = ArrayVec::new();
        if let Some(p) = curr_pos {
            curr_pos_properties.extend(format_speed_and_heading(p));
            curr_pos_properties.extend(format_pos(&Position::new(p)));
        }
        let saved_positions = model
            .view_saved_positions
            .clone()
            .into_iter()
            .map(|p| ViewSavedPos::new(p, curr_pos.map(|x| x.coords)))
            .collect();
        Self {
            gps_status,
            curr_pos_properties,
            saved_positions,
            msg: if model.msg.is_empty() {
                None
            } else {
                Some(model.msg.clone())
            },
        }
    }
}
