//! Traits for geo types.

use chrono::{DateTime, Utc};
use crux_geolocation::GeoInfo;
use jord::{LatLong, Length, NVector};

/// A trait for position types which has coordinates.
pub trait Coords {
    fn coords(&self) -> LatLong;
    /// Get the coordinates as an `NVector`.
    fn nvector(&self) -> NVector {
        self.coords().to_nvector()
    }
    fn accuracy(&self) -> Option<Length> {
        None
    }
}

impl Coords for LatLong {
    fn coords(&self) -> LatLong {
        *self
    }
}

impl Coords for NVector {
    fn coords(&self) -> LatLong {
        LatLong::from_nvector(*self)
    }
}

impl Coords for GeoInfo {
    fn coords(&self) -> LatLong {
        self.coords
    }
    fn accuracy(&self) -> Option<Length> {
        self.accuracy
    }
}

/// Altitude information.
pub trait Altitude {
    fn altitude(&self) -> Option<Length>;
    fn altitude_accuracy(&self) -> Option<Length>;
}

impl Altitude for GeoInfo {
    fn altitude(&self) -> Option<Length> {
        self.altitude
    }
    fn altitude_accuracy(&self) -> Option<Length> {
        self.altitude_accuracy
    }
}

/// A recorded position with coordinates, altitude, accuracy and timestamp.
pub trait RecordedPos: Coords + Altitude {
    fn timestamp(&self) -> DateTime<Utc>;
}

impl RecordedPos for GeoInfo {
    fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
}
