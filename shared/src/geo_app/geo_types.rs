use std::ops::Div;
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use crux_geolocation::GeoInfo;
use ecow::EcoString;
use jord::{
    LatLong, Length, Measurement, NVector, Vec3,
    spherical::{GreatCircle, MinorArc},
};
use rstar::{AABB, PointDistance, RTreeObject};
use serde::{Deserialize, Serialize};

use super::geo_traits::*;
use crate::PLANET;
use crate::numbers::eq_zero;

/// A position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub coords: LatLong,
    pub altitude: Option<Length>,
    pub accuracy: Option<Length>,
    pub altitude_accuracy: Option<Length>,
}

impl<T: Coords + Altitude> From<&T> for Position {
    fn from(x: &T) -> Self {
        Self {
            coords: x.coords(),
            altitude: x.altitude(),
            accuracy: x.accuracy(),
            altitude_accuracy: x.altitude_accuracy(),
        }
    }
}

impl Coords for Position {
    fn coords(&self) -> LatLong {
        self.coords
    }
    fn accuracy(&self) -> Option<Length> {
        self.accuracy
    }
}

impl Altitude for Position {
    fn altitude(&self) -> Option<Length> {
        self.altitude
    }
    fn altitude_accuracy(&self) -> Option<Length> {
        self.altitude_accuracy
    }
}

/// A position with a timestamp.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PosWithTimestamp {
    pub pos: Position,
    pub timestamp: DateTime<Utc>,
}

impl<T: RecordedPos> From<&T> for PosWithTimestamp {
    fn from(x: &T) -> Self {
        Self {
            pos: x.into(),
            timestamp: x.timestamp(),
        }
    }
}

impl Coords for PosWithTimestamp {
    fn coords(&self) -> LatLong {
        self.pos.coords()
    }
    fn accuracy(&self) -> Option<Length> {
        self.pos.accuracy()
    }
}

impl Altitude for PosWithTimestamp {
    fn altitude(&self) -> Option<Length> {
        self.pos.altitude()
    }
    fn altitude_accuracy(&self) -> Option<Length> {
        self.pos.altitude_accuracy()
    }
}

impl RecordedPos for PosWithTimestamp {
    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// A saved position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedPos {
    pub name: EcoString,
    pub pos: Position,
    pub timestamp: DateTime<Utc>,
}

impl SavedPos {
    pub fn new(name: EcoString, geo: &GeoInfo) -> Self {
        Self {
            name,
            pos: geo.into(),
            timestamp: geo.timestamp,
        }
    }
}

impl Coords for SavedPos {
    fn coords(&self) -> LatLong {
        self.pos.coords
    }
    fn accuracy(&self) -> Option<Length> {
        self.pos.accuracy
    }
}

impl Altitude for SavedPos {
    fn altitude(&self) -> Option<Length> {
        self.pos.altitude
    }
    fn altitude_accuracy(&self) -> Option<Length> {
        self.pos.altitude_accuracy
    }
}

impl RecordedPos for SavedPos {
    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}

/// Get a point passed to `RTree`.
pub fn rtree_point<T: Coords>(pos: &T) -> [f64; 3] {
    let nvec = pos.nvector().as_vec3();
    [nvec.x(), nvec.y(), nvec.z()]
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
        AABB::from_point(rtree_point(self))
    }
}

impl PointDistance for SavedPos {
    fn distance_2(&self, point: &[f64; 3]) -> f64 {
        let me = rtree_point(self);
        let [x, y, z] = [me[0] - point[0], me[1] - point[1], me[2] - point[2]];
        return x * x + y * y + z * z;
    }
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
pub struct Way<T> {
    /// An ordered list of positions.
    nodes: Vec<T>,
    /// The length of the way.
    length: Length,
    /// An **ordered** list of all accuracies for all positions. Used to compute statistical
    /// metrics like median or other percentiles. (Lazily initialised.)
    ///
    /// This is basically a order statistic tree (see Wikipedia). When inserting elements, you may
    /// only use the [`imbl::Vector::insert_ord_by()`] method to preserve ordering.
    ///
    /// The order is determined by [`f64::total_cmp()`] on [`Length::as_default_unit()`].
    #[serde(skip)]
    accuracies: OnceLock<imbl::Vector<Length>>,
}

impl<T> Way<T> {
    pub fn new() -> Self {
        Self {
            nodes: vec![],
            length: Length::ZERO,
            accuracies: OnceLock::new(),
        }
    }

    pub fn nodes(&self) -> &[T] {
        &self.nodes
    }

    pub fn length(&self) -> Length {
        self.length
    }
}

impl<T: Coords> Way<T> {
    /// Add a node to the end of the way.
    pub fn append(&mut self, pos: T) {
        // Adjust the length.
        if let Some(last) = self.nodes.last() {
            self.length = self.length + PLANET.distance(last.nvector(), pos.nvector());
        }

        // Adjust `self.accuracies` if it is initialised and `pos.accuracy()` returns `Some`.
        if let (Some(accuracies), Some(accuracy)) = (self.accuracies.get_mut(), pos.accuracy()) {
            accuracies.insert_ord_by(accuracy, |x, y| {
                x.as_default_unit().total_cmp(&y.as_default_unit())
            });
        }

        self.nodes.push(pos);
    }

    /// Insert a node at the specified index.
    pub fn insert(&mut self, i: usize, pos: T) {
        // Adjust `self.accuracies` if it is initialised and `pos.accuracy()` returns `Some`.
        if let (Some(accuracies), Some(accuracy)) = (self.accuracies.get_mut(), pos.accuracy()) {
            accuracies.insert_ord_by(accuracy, |x, y| {
                x.as_default_unit().total_cmp(&y.as_default_unit())
            });
        }

        self.nodes.insert(i, pos);
        self.recompute_length()
    }

    /// Change a node at a certain index.
    pub fn update(&mut self, i: usize, new_pos: T) {
        // Change the accuracy for the point if needed.
        if let Some(accuracies) = self.accuracies.get_mut() {
            // Remove the accuracy for the position that is gonna be removed.
            if let Some(old_accuracy) = self.nodes[i].accuracy() {
                let Ok(idx_of_old_accuracy) = accuracies.binary_search_by(|other| {
                    old_accuracy
                        .as_default_unit()
                        .total_cmp(&other.as_default_unit())
                }) else {
                    panic!("The accuracy for an existing node must appear in self.accuracies.");
                };
                accuracies.remove(idx_of_old_accuracy);
            }

            // Insert the accuracy for the new pos.
            if let Some(new_accuracy) = new_pos.accuracy() {
                accuracies.insert_ord_by(new_accuracy, |x, y| {
                    x.as_default_unit().total_cmp(&y.as_default_unit())
                });
            }
        }

        self.nodes[i] = new_pos;
        self.recompute_length();
    }

    /// Recompute the length for the way.
    fn recompute_length(&mut self) {
        self.length = Length::ZERO;
        for i in 1..self.nodes.len() {
            self.length =
                self.length + PLANET.distance(self.nodes[i - 1].nvector(), self.nodes[i].nvector());
        }
    }

    /// Compute the value for [`Self::accuracies`]. This function is meant to be passed to
    /// `self.accuracies.get_or_init()` or `self.accuracies.get_mut_or_init()`.
    fn compute_accuracies(&self) -> imbl::Vector<Length> {
        let mut accuracies = self
            .nodes()
            .iter()
            .filter_map(|p| p.accuracy())
            .collect::<imbl::Vector<Length>>();
        accuracies.sort_by(|x, y| x.as_default_unit().total_cmp(&y.as_default_unit()));
        accuracies
    }

    /// Get a sorted o list of all accuracies for nodes in this [`Way`].
    ///
    /// Note that it probably contain duplicates and it may be shorter than [`Self::nodes()`] if
    /// some or all of the nodes doesn't provide an accuracy.
    pub fn accuracies(&self) -> &imbl::Vector<Length> {
        self.accuracies.get_or_init(|| self.compute_accuracies())
    }

    /// Get the median accuracy.
    pub fn median_accuracy(&self) -> Option<Length> {
        let accuracies = self.accuracies();
        if accuracies.len() > 0 {
            if accuracies.len() % 2 == 1 {
                Some(accuracies[accuracies.len() / 2])
            } else {
                let idx = accuracies.len() / 2;
                let mut accuracies = accuracies.focus();
                Some((*accuracies.index(idx) + *accuracies.index(idx - 1)) / 2.0)
            }
        } else {
            None
        }
    }
}

/// A recorded way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RecordedWay {
    pub way: Way<PosWithTimestamp>,
}

impl RecordedWay {
    pub fn new() -> Self {
        Self { way: Way::new() }
    }

    pub fn way(&self) -> &Way<impl RecordedPos> {
        &self.way
    }

    /// Add a point to the recording.
    pub fn add(&mut self, pos: &impl RecordedPos) {
        if self
            .way
            .nodes()
            .last()
            .map(|x| x.timestamp < pos.timestamp())
            .unwrap_or(true)
        {
            self.way.append(pos.into());
        } else {
            match self
                .way
                .nodes()
                .binary_search_by_key(&pos.timestamp(), RecordedPos::timestamp)
            {
                Err(i) => {
                    self.way.insert(i, pos.into());
                }
                Ok(i) => {
                    // A node with the same timestamp is already saved, so we will replace it.
                    self.way.update(i, pos.into());
                }
            }
        }
    }

    /// Get all positions since a certain timestamp. (Inclusive)
    pub fn get_since(&self, timestamp: DateTime<Utc>) -> &[PosWithTimestamp] {
        let i = self
            .way
            .nodes()
            .binary_search_by_key(&timestamp, RecordedPos::timestamp)
            .unwrap_or_else(|i| i);
        &self.way.nodes()[i..]
    }
}

#[cfg(test)]
mod tests {
    use itertools::iproduct;
    use jord::{Angle, spherical::Sphere};

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
