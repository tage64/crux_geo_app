use chrono::{DateTime, Utc};
use crux_core::{
    Request,
    capability::Operation,
    command::{Command, NotificationBuilder, StreamBuilder},
};
use futures::Stream;
use jord::{Angle, LatLong, Length, Speed};
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;

/// The coordinates, altitude, speed and bearing of a device. (This type is used only by the shell
/// and not the app.)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    /// The latitude in decimal degrees.
    pub latitude: f64,
    /// The longitude in decimal degrees.
    pub longitude: f64,
    /// The altitude in meters, relative to nominal sea level. (Optional)
    pub altitude: Option<f64>,
    /// The accuracy of latitude and longitude in meters. (Optional).
    pub accuracy: Option<f64>,
    /// The accuracy of the altitude in meters. (Optional)
    pub altitude_accuracy: Option<f64>,
    /// The direction towards which the device is facing. (Optional)
    ///
    /// This value, specified in degrees, indicates how far off from heading true north the device
    /// is. 0 degrees represents true north, and the direction is determined clockwise (which means
    /// that east is 90 degrees and west is 270 degrees). If speed is 0 or the device is unable to
    /// provide heading information, heading is `None`.
    pub heading: Option<f64>,
    /// The velocity of the device in meters per second. (Optional)
    pub volocity: Option<f64>,
}

/// Options when retrieving a position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoOptions {
    /// A positive value indicating the maximum age in milliseconds of a possible cached
    /// position that is acceptable to return.
    ///
    /// If set to 0, it means that the device cannot use a cached position and must attempt to
    /// retrieve the real current position.
    pub maximum_age: u64,
    /// A positive value representing the maximum length of time (in milliseconds) the device is
    /// allowed to take in order to return a position.
    ///
    /// `None` means that the device will not return until the position is availlable.
    pub timeout: Option<u64>,
    /// A bool that indicates the application would like to receive the best possible results.
    ///
    /// If true and if the device is able to provide a more accurate position, it will do
    /// so. Note that this can result in slower response times or increased power consumption (with
    /// a GPS chip on a mobile device for example). On the other hand, if false, the device can
    /// take the liberty to save resources by responding more quickly and/or using less power.
    /// Default: false.
    pub enable_high_accuracy: bool,
}

/// A position operation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GeoOperation {
    WatchPosition(GeoOptions),
    ClearWatch,
}

/// An error which may occur when retrieving the current position.
#[derive(
    Clone, Debug, PartialEq, Serialize, Deserialize, derive_more::Display, derive_more::Error,
)]
#[serde(rename_all = "camelCase")]
#[repr(u8)]
pub enum GeoError {
    #[display("Permission denied")]
    PermissionDenied = 1,
    #[display("Position unavailable")]
    PositionUnavailable = 2,
    /// The time allowed to acquire the position was reached before the information was obtained.
    #[display("Position retrieval timed out")]
    Timeout = 3,
}

pub type GeoResult<T, E = GeoError> = Result<T, E>;

/// A position response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GeoResponse {
    Position {
        /// The location.
        coords: Position,
        /// The time when the location was retrieved as Unix time in milliseconds.
        timestamp: i64,
    },
    PermissionDeniedError,
    PositionUnavailableError,
    TimeoutError,
}

impl Operation for GeoOperation {
    type Output = GeoResponse;
}

/// The coordinates, altitude, speed and bearing of a device. (This type is used only by the shell
/// and not the app.)
#[derive(Clone, Debug, PartialEq)]
pub struct GeoInfo {
    /// The timestamp when the position was retrieved.
    pub timestamp: DateTime<Utc>,
    /// The latitude and longitude of the device on earth.
    pub coords: LatLong,
    /// The altitude of the device, relative to nominal sea level. (Optional)
    pub altitude: Option<Length>,
    /// The accuracy of latitude and longitude. (Optional).
    pub accuracy: Option<Length>,
    /// The accuracy of the altitude. (Optional)
    pub altitude_accuracy: Option<Length>,
    /// The direction towards which the device is facing. (Optional)
    ///
    /// This value specifies how far off from heading true north the device is. 0 degrees
    /// represents true north, and the direction is determined clockwise (which means that east is
    /// 90 degrees and west is 270 degrees). If speed is 0 or the device is unable to provide
    /// heading information, heading is `None`.
    pub bearing: Option<Angle>,
    /// The velocity of the device. (Optional)
    pub volocity: Option<Speed>,
}

/// The Geolocation capability API
///
/// This capability provides access to the current location and allows the app to watch position
/// updates.
#[derive(Clone)]
pub struct Geolocation<Effect, Event> {
    effect: PhantomData<Effect>,
    event: PhantomData<Event>,
}

impl<Effect, Event> Geolocation<Effect, Event>
where
    Effect: Send + From<Request<GeoOperation>> + 'static,
    Event: Send + 'static,
{
    /// Watch the current position and stream when the position changes.
    ///
    /// Any existing watch will be cleared.
    pub fn watch_position(
        options: GeoOptions,
    ) -> StreamBuilder<Effect, Event, impl Stream<Item = GeoResult<GeoInfo>>> {
        Command::stream_from_shell(GeoOperation::WatchPosition(options)).map(response_to_geo_info)
    }

    /// Cancel any existing position watcher.
    ///
    /// If no watcher is active, this method does nothing.
    pub fn clear_watch() -> NotificationBuilder<Effect, Event, impl Future<Output = ()>> {
        Command::notify_shell(GeoOperation::ClearWatch)
    }
}

fn response_to_geo_info(response: GeoResponse) -> GeoResult<GeoInfo> {
    match response {
        GeoResponse::Position {
            timestamp,
            coords:
                Position {
                    latitude,
                    longitude,
                    altitude,
                    accuracy,
                    altitude_accuracy,
                    heading,
                    volocity,
                },
        } => Ok(GeoInfo {
            timestamp: DateTime::from_timestamp_millis(timestamp)
                .expect("Failed to create timestamp from millis."),
            coords: LatLong::from_degrees(latitude, longitude),
            altitude: altitude.map(Length::from_metres),
            accuracy: accuracy.map(Length::from_metres),
            altitude_accuracy: altitude_accuracy.map(Length::from_metres),
            bearing: heading.map(Angle::from_degrees),
            volocity: volocity.map(Speed::from_metres_per_second),
        }),
        GeoResponse::PermissionDeniedError => Err(GeoError::PermissionDenied),
        GeoResponse::PositionUnavailableError => Err(GeoError::PositionUnavailable),
        GeoResponse::TimeoutError => Err(GeoError::Timeout),
    }
}
