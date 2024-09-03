use std::time::Duration;

use chrono::{DateTime, Utc};
use compact_str::CompactString;
use crux_geolocation::GeoInfo;
use jord::{LatLong, Length};
use rstar::{PointDistance, RTreeObject, AABB};
use serde::{Deserialize, Serialize};

/// A position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub coords: LatLong,
    pub altitude: Option<Length>,
    pub accuracy: Option<Length>,
    pub altitude_accuracy: Option<Length>,
}

impl Position {
    pub fn new(geo: &GeoInfo) -> Self {
        Self {
            coords: geo.coords,
            altitude: geo.altitude,
            accuracy: geo.accuracy,
            altitude_accuracy: geo.altitude_accuracy,
        }
    }
}

/// A saved position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedPos {
    pub name: CompactString,
    pub pos: Position,
    pub timestamp: DateTime<Utc>,
}

impl SavedPos {
    pub fn new(name: CompactString, geo: &GeoInfo) -> Self {
        Self {
            name,
            pos: Position::new(geo),
            timestamp: geo.timestamp,
        }
    }
}

/// We implement RTreeObject for a position on the Earth's surface. (Ignoring altitude.)
///
/// The distances will technically not be correct since the rtree will compute the direct distance
/// through the Earth without following the surface. This doesn't matter though because the
/// comparisons are equivalent. That is, point a is closer than point b to point c following the
/// surface if and only if point a is closer than point b to point c in a 3d cartesian system.
///
/// Since the distances are messed up anyway, we will use the unit normal vector to the surface to
/// represent a point. That is, the actual vector (from the center of the Earth) devided by the
/// Earth's radius.
impl RTreeObject for SavedPos {
    type Envelope = AABB<[f64; 3]>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(rtree_point(self.pos.coords))
    }
}

impl PointDistance for SavedPos {
    fn distance_2(&self, point: &[f64; 3]) -> f64 {
        let me = rtree_point(self.pos.coords);
        let [x, y, z] = [me[0] - point[0], me[1] - point[1], me[2] - point[2]];
        return x * x + y * y + z * z;
    }
}

/// Get a point passed to `RTree`.
pub fn rtree_point(coords: LatLong) -> [f64; 3] {
    let nvec = coords.to_nvector().as_vec3();
    [nvec.x(), nvec.y(), nvec.z()]
}

/// A list of positions, preferably forming a natural path on the map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Way {
    /// An ordered list of positions.
    pub nodes: Vec<Position>,
}

/// A way which is being recorded.
#[derive(Debug, Clone)]
pub(crate) struct RecordedWay {
    pub way: Way,
    /// Timestamps for each node in the way. Must start with 0 and be monotonically increasing.
    pub timestamps: Vec<Duration>,
}

impl RecordedWay {
    /// Start a recording.
    pub fn start(pos: Position) -> Self {
        Self {
            way: Way { nodes: vec![pos] },
            timestamps: vec![Duration::ZERO],
        }
    }

    /// Add a point to the recording.
    pub fn push(&mut self, pos: Position) {
        self.timestamps.push(Duration::ZERO);
        self.way.nodes.push(pos);
    }

    /// Get all positions from now minus a certain duration.
    ///
    /// Returns a tuple of positions and durations since the start of the recording.
    pub fn get_last(&self, duration: Duration) -> (&[Position], &[Duration]) {
        // TODO: Fix this.
        // let i = self
        // .timestamps
        // .binary_search(&(self.started_at.duration_since(Instant::now()) - duration))
        // .unwrap_or_else(|x| x);
        let i = 0;
        (&self.way.nodes[i..], &self.timestamps[i..])
    }
}
