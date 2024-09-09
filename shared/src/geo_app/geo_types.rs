use chrono::{DateTime, Utc};
use compact_str::CompactString;
use crux_geolocation::GeoInfo;
use jord::{LatLong, Length, NVector};
use rstar::{PointDistance, RTreeObject, AABB};
use serde::{Deserialize, Serialize};

use crate::PLANET;

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

/// A line is actually a minor arc (or a geodesi) on the surface of the planet.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Line {
    start: NVector,
    end: NVector,
}

impl Line {
    pub fn new(start: LatLong, end: LatLong) -> Self {
        Self {
            start: start.to_nvector(),
            end: end.to_nvector(),
        }
    }
}

/// A list of positions, preferably forming a natural path on the map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Way {
    /// An ordered list of positions.
    nodes: Vec<Position>,
    /// The length of the way.
    length: Length,
}

impl Way {
    pub fn new() -> Self {
        Self {
            nodes: vec![],
            length: Length::ZERO,
        }
    }

    pub fn nodes(&self) -> &[Position] {
        &self.nodes
    }

    pub fn length(&self) -> Length {
        self.length
    }

    /// Add a node to the end of the way.
    pub fn append(&mut self, pos: Position) {
        if let Some(last) = self.nodes.last() {
            self.length =
                self.length + PLANET.distance(last.coords.to_nvector(), pos.coords.to_nvector());
        }
        self.nodes.push(pos);
    }

    /// Insert a node at the specified index.
    pub fn insert(&mut self, i: usize, pos: Position) {
        self.nodes.insert(i, pos);
        self.recompute_length()
    }

    /// Change a node at a certain index.
    pub fn update(&mut self, i: usize, new_pos: Position) {
        self.nodes[i] = new_pos;
        self.recompute_length();
    }

    /// Recompute the length for the way.
    fn recompute_length(&mut self) {
        self.length = Length::ZERO;
        for i in 1..self.nodes.len() {
            self.length = self.length
                + PLANET.distance(
                    self.nodes[i - 1].coords.to_nvector(),
                    self.nodes[i].coords.to_nvector(),
                );
        }
    }
}

/// A way which is being recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RecordedWay {
    pub way: Way,
    /// Timestamps for each node in the way. Must be monotonically increasing.
    pub timestamps: Vec<DateTime<Utc>>,
}

impl RecordedWay {
    pub fn new() -> Self {
        Self {
            way: Way::new(),
            timestamps: vec![],
        }
    }

    /// Add a point to the recording.
    pub fn add(&mut self, geo: &GeoInfo) {
        if self
            .timestamps
            .last()
            .map(|x| x < &geo.timestamp)
            .unwrap_or(true)
        {
            self.timestamps.push(geo.timestamp);
            self.way.append(Position::new(geo));
        } else {
            match self.timestamps.binary_search(&geo.timestamp) {
                Err(i) => {
                    self.timestamps.insert(i, geo.timestamp);
                    self.way.insert(i, Position::new(geo));
                }
                Ok(i) => {
                    // A node with the same timestamp is already saved, so we will replace it.
                    self.timestamps[i] = geo.timestamp;
                    self.way.update(i, Position::new(geo));
                }
            }
        }
    }

    /// Get all positions since a certain timestamp. (Inclusive)
    pub fn get_since(&self, timestamp: DateTime<Utc>) -> (&[Position], &[DateTime<Utc>]) {
        let i = self
            .timestamps
            .binary_search(&timestamp)
            .unwrap_or_else(|i| i);
        (&self.way.nodes[i..], &self.timestamps[i..])
    }
}
