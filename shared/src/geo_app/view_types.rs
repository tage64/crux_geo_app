//! Some types to store information to be viewed by the UI.
//!
//! The general theme is that these types formats numbers and units to strings so that the UI don't
//! have to bother with that.
use chrono::Local;
use compact_str::{format_compact, CompactString, ToCompactString};
use crux_geolocation::GeoInfo;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use super::SavedPos;

/// Information about a position.
///
/// The information fields are represented by strings, including the value and the unit but not the
/// name. E.g. "59.265358° North", but the name "Latitude: " is not included.
#[derive(Serialize, Deserialize, Clone, Default)]
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
        let rounded_coords = self.coords.round_d5(); // Meter precition.
        ViewPos {
            name: self.name.clone(),
            latitude: format_compact!("{}°", rounded_coords.latitude().as_degrees()),
            longitude: format_compact!("{}°", rounded_coords.longitude().as_degrees()),
            altitude: self
                .altitude
                .map(|x| format_compact!("{} m", x.as_metres())),
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
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ViewVolocity {
    pub volocity: CompactString,
    pub bearing: Option<CompactString>,
}

impl ViewVolocity {
    pub fn new(geo: &GeoInfo) -> Option<Self> {
        if let Some(volocity) = geo.volocity {
            Some(Self {
                volocity: format_compact!("{} m/s", volocity.as_metres_per_second()),
                bearing: geo.bearing.map(|x| format_compact!("{}°", x.as_degrees())),
            })
        } else {
            None
        }
    }
}

/// The entire view model. This is everything sent to the UI.
#[derive(Serialize, Deserialize, Clone, Default)]
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
}
