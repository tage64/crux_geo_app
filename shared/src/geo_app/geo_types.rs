use std::ops::Div;

use chrono::{DateTime, Utc};
use compact_str::CompactString;
use crux_geolocation::GeoInfo;
use jord::{
    spherical::{GreatCircle, MinorArc},
    LatLong, Length, NVector, Vec3,
};
use rstar::{PointDistance, RTreeObject, AABB};
use serde::{Deserialize, Serialize};

use crate::numbers::eq_zero;
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
pub struct Line(MinorArc);

impl Line {
    pub fn new(start: LatLong, end: LatLong) -> Self {
        Line(MinorArc::new(start.to_nvector(), end.to_nvector()))
    }

    /// Compute the min and max points on this line with respect to a certain direction.
    ///
    /// `direction` **must** be a unit length vector.
    ///
    /// Returns a tuple (min, max).
    fn extrema(&self, direction: Vec3) -> (f64, f64) {
        let n = self.0.normal();
        // m is orthogonal to the normal and the direction.
        let m = n.cross_prod(direction);
        // ms and me are the dot products between m and the start and end respectively.
        let ms = m.dot_prod(self.0.start().as_vec3());
        let me = m.dot_prod(self.0.end().as_vec3());
        let start_height = direction.dot_prod(self.0.start().as_vec3());
        let end_height = direction.dot_prod(self.0.end().as_vec3());
        let (min_p, max_p) = if start_height < end_height {
            (start_height, end_height)
        } else {
            (end_height, start_height)
        };
        if ms * me >= 0.0 || eq_zero(ms) || eq_zero(me) {
            (min_p, max_p)
        } else if ms < 0.0 {
            (min_p, direction.cross_prod(n).norm())
        } else {
            (-direction.cross_prod(n).norm(), max_p)
        }
    }
}

impl RTreeObject for Line {
    type Envelope = AABB<[f64; 3]>;
    fn envelope(&self) -> Self::Envelope {
        let (x_min, x_max) = self.extrema(Vec3::UNIT_X);
        let (y_min, y_max) = self.extrema(Vec3::UNIT_Y);
        let (z_min, z_max) = self.extrema(Vec3::UNIT_Z);
        AABB::from_corners([x_min, y_min, z_min], [x_max, y_max, z_max])
    }
}

impl PointDistance for Line {
    fn distance_2(&self, point: &[f64; 3]) -> f64 {
        let point = NVector::new(Vec3::new(point[0], point[1], point[2]));
        f64::min(
            PLANET
                .cross_track_distance(point, GreatCircle::new(self.0.start(), self.0.end()))
                .as_metres()
                .div(PLANET.radius().as_metres())
                .powi(2),
            f64::min(
                PLANET
                    .distance(point, self.0.start())
                    .as_metres()
                    .div(PLANET.radius().as_metres())
                    .powi(2),
                PLANET
                    .distance(point, self.0.end())
                    .as_metres()
                    .div(PLANET.radius().as_metres())
                    .powi(2),
            ),
        )
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

#[cfg(test)]
mod tests {
    use itertools::iproduct;
    use jord::{spherical::Sphere, Angle};

    use super::*;
    use crate::numbers::{gte, lte};

    #[test]
    fn test_line_extrema() {
        let angles = [
            Angle::ZERO,
            Angle::QUARTER_CIRCLE,
            Angle::HALF_CIRCLE,
            Angle::NEG_HALF_CIRCLE,
            Angle::NEG_QUARTER_CIRCLE,
        ];
        let directions = [Vec3::UNIT_X, Vec3::UNIT_Y, Vec3::UNIT_Z];
        let fractions = [0.0, 1.0 / 4.0, 1.0 / 3.0, 1.0 / 2.0, 1.0];
        for (lat_1, lat_2, long_1, long_2, direction) in
            iproduct!(angles, angles, angles, angles, directions)
        {
            let line = Line::new(LatLong::new(lat_1, long_1), LatLong::new(lat_2, long_2));
            let (min, max) = line.extrema(direction);
            assert!(-1.0 <= min);
            assert!(max <= 1.0);
            assert!(min <= max);
            let start_height = direction.dot_prod(line.0.start().as_vec3());
            let end_height = direction.dot_prod(line.0.end().as_vec3());
            let (min_p, max_p) = if start_height < end_height {
                (start_height, end_height)
            } else {
                (end_height, start_height)
            };
            assert!(min <= min_p);
            assert!(max_p <= max);
            for fraction in fractions {
                let point =
                    Sphere::interpolated_pos(line.0.start(), line.0.end(), fraction).unwrap();
                assert!(line.0.contains_point(point));
                let height = direction.dot_prod(point.as_vec3());
                assert!(lte(height, max));
                assert!(gte(height, min));
            }
        }
    }
}
