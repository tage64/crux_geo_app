//! Some types to store information to be viewed by the UI.
//!
//! The general theme is that these types formats numbers and units to strings so that the UI don't
//! have to bother with that.
use arrayvec::ArrayVec;
use chrono::prelude::*;
use compact_str::{format_compact, CompactString, ToCompactString};
use crux_geolocation::GeoInfo;
use jord::{spherical::Sphere, LatLong, Length};
use serde::{Deserialize, Serialize};

use super::{Model, SavedPos, PLANET};

/// Precition for latitude and longitude.
const COORD_PRECITION: usize = 5;
/// Precition for altitude, volocity and other things.
const PRECITION: usize = 1;

/// Basic information about a position.
///
/// The information fields are represented by strings, including the value and the unit but not the
/// name. E.g. "59.265358° North", but the name "Latitude: " is not included.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
pub struct ViewPos {
    /// The latitude in decimal degrees.
    pub latitude: CompactString,
    /// The longitude in decimal degrees.
    pub longitude: CompactString,
    /// The altitude in decimal degrees.
    pub altitude: Option<CompactString>,
    /// The time when the position was captured.
    pub timestamp: CompactString,
}

/// Information about a named position.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
pub struct ViewNamedPos {
    /// The name and (if it does exist) a distance and direction.
    pub summary: CompactString,
    /// A number of properties, like latitude and timestamp.
    pub properties: ArrayVec<CompactString, 4>,
}

impl ViewPos {
    pub fn new(coords: LatLong, altitude: Option<Length>, timestamp: DateTime<Utc>) -> Self {
        let latitude = coords.latitude().as_degrees();
        let longitude = coords.longitude().as_degrees();
        let north_south = if latitude >= 0.0 { "North" } else { "South" };
        let east_west = if longitude >= 0.0 { "East" } else { "West" };
        Self {
            latitude: format_compact!("{:.*}° {}", COORD_PRECITION, latitude, north_south),
            longitude: format_compact!("{:.*}° {}", COORD_PRECITION, longitude, east_west,),
            altitude: altitude.map(|x| format_compact!("{:.*} meters", PRECITION, x.as_metres())),
            timestamp: timestamp
                .with_timezone(&Local)
                .format("%a %b %e %T %Y")
                .to_compact_string(),
        }
    }
}

impl ViewNamedPos {
    fn new(pos: SavedPos, curr_pos: Option<LatLong>) -> Self {
        let summary = if let Some(curr_coords) = curr_pos {
            format_compact!(
                "{}: {} m, {}°",
                pos.name,
                PLANET
                    .distance(curr_coords.to_nvector(), pos.coords.to_nvector())
                    .as_metres()
                    .round(),
                Sphere::initial_bearing(curr_coords.to_nvector(), pos.coords.to_nvector())
                    .as_degrees()
                    .round()
            )
        } else {
            pos.name
        };

        let ViewPos {
            latitude,
            longitude,
            altitude,
            timestamp,
        } = ViewPos::new(pos.coords, pos.altitude, pos.timestamp);
        let mut properties = ArrayVec::new();
        properties.push(format_compact!("Latitude: {latitude}"));
        properties.push(format_compact!("Longitude: {longitude}"));
        if let Some(altitude) = altitude {
            properties.push(format_compact!("Altitude: {altitude}"));
        }
        properties.push(format_compact!("Saved at: {timestamp}"));
        Self {
            summary,
            properties,
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
    /// Saved positions to show.
    pub saved_positions: Vec<ViewNamedPos>,
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
        let curr_pos: Option<&GeoInfo> = model.curr_pos.as_ref().map(|x| x.as_ref().ok()).flatten();
        let saved_positions = model
            .view_saved_positions
            .clone()
            .into_iter()
            .map(|p| ViewNamedPos::new(p, curr_pos.map(|x| x.coords)))
            .collect();
        Self {
            volocity: curr_pos.map(ViewVolocity::new).flatten(),
            curr_pos: curr_pos.map(|p| ViewPos::new(p.coords, p.altitude, p.timestamp)),
            saved_positions,
            gps_status,
            msg: if model.msg.is_empty() {
                None
            } else {
                Some(model.msg.clone())
            },
        }
    }
}
