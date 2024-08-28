use crux_core::capability::{CapabilityContext, Operation};
use futures::{Stream, StreamExt as _};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// The coordinates, altitude, speed and bearing of a device.
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
    pub heading: f64,
    /// The velocity of the device in meters per second, (Optional)
    pub volocity: f64,
}

/// Options when retrieving a position.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    /// A positive value indicating the maximum age in milliseconds of a possible cached
    /// position that is acceptable to return.
    ///
    /// If set to 0, it means that the device cannot use a cached position and must attempt to
    /// retrieve the real current position. If set to Infinity the device must return a cached
    /// position regardless of its age. Default: 0.
    pub maximum_age: f64,
    /// A positive value representing the maximum length of time (in milliseconds) the device is
    /// allowed to take in order to return a position.
    ///
    /// `None` means that the device will not return until the position is availlable.
    pub timeout: Option<f64>,
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Request {
    GetCurrentPosition(Options),
    WatchPosition(Options),
    ClearWatch(i64),
}

/// An error which may occur when retrieving the current position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[repr(u8)]
pub enum Error {
    PermissionDenied = 1,
    PositionUnavaillable = 2,
    /// The time allowed to acquire the position was reached before the information was obtained.
    Timeout = 3,
}

/// A position response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Response {
    Position {
        /// The location.
        coords: Position,
        /// The time when the location was retrieved as Unix time in milliseconds.
        timestamp: i64,
    },
    Error(Error),
}

impl Operation for Request {
    type Output = Response;
}

/// The Geolocation capability API
///
/// This capability provides access to the current location and allows the app to watch position
/// updates.
pub struct Geolocation<Ev> {
    context: CapabilityContext<Request, Ev>,
    /// An id of current watch, used to clear the watch.
    watch_id: Arc<Mutex<Option<i64>>>,
}

impl<Ev> Clone for Geolocation<Ev> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            watch_id: self.watch_id.clone(),
        }
    }
}

impl<Ev> crux_core::Capability<Ev> for Geolocation<Ev> {
    type Operation = Request;
    type MappedSelf<MappedEv> = Geolocation<MappedEv>;

    fn map_event<F, NewEv>(&self, f: F) -> Self::MappedSelf<NewEv>
    where
        F: Fn(NewEv) -> Ev + Send + Sync + 'static,
        Ev: 'static,
        NewEv: 'static + Send,
    {
        Geolocation::new(self.context.map_event(f))
    }

    #[cfg(feature = "typegen")]
    fn register_types(generator: &mut crux_core::typegen::TypeGen) -> crux_core::typegen::Result {
        generator.register_type::<Position>()?;
        generator.register_type::<Options>()?;
        generator.register_type::<Error>()?;
        generator.register_type::<Self::Operation>()?;
        generator.register_type::<<Self::Operation as Operation>::Output>()?;
        Ok(())
    }
}
impl<Ev> Geolocation<Ev>
where
    Ev: 'static,
{
    pub fn new(context: CapabilityContext<Request, Ev>) -> Self {
        Self {
            context,
            watch_id: Arc::new(Mutex::new(None)),
        }
    }

    /// Request the current position.
    pub fn get_position<F>(&self, options: Options, callback: F)
    where
        F: FnOnce(Response) -> Ev + Send + Sync + 'static,
    {
        self.context.spawn({
            let context = self.context.clone();
            let this = self.clone();

            async move {
                context.update_app(callback(this.get_position_async(options).await));
            }
        });
    }

    /// Request the current position.
    ///
    /// This is an async call to use with [`crux_core::compose::Compose`].
    pub async fn get_position_async(&self, options: Options) -> Response {
        self.context
            .request_from_shell(Request::GetCurrentPosition(options))
            .await
    }

    /// Watch the current position and stream when the position changes.
    ///
    /// Any existing watch will be cleared.
    pub fn watch_position<F>(&self, options: Options, mut callback: F)
    where
        F: FnMut(Response) -> Ev + Send + Sync + 'static,
    {
        self.context.spawn({
            let context = self.context.clone();
            let this = self.clone();

            async move {
                this.watch_position_async(options)
                    .await
                    .map(|x| context.update_app(callback(x)))
                    .collect::<()>()
                    .await;
            }
        });
    }

    /// Request the current position.
    ///
    /// This is an async call to use with [`crux_core::compose::Compose`].
    pub async fn watch_position_async(&self, options: Options) -> impl Stream<Item = Response> {
        // Clear earlier watch.
        self.clear_watch_async().await;
        self.context
            .stream_from_shell(Request::GetCurrentPosition(options))
    }

    /// Cancel any existing position watcher.
    pub fn clear_watch(&self) {
        self.context.spawn({
            let this = self.clone();
            async move { this.clear_watch_async().await }
        });
    }

    /// Cancel any existing position watcher.
    ///
    /// If no watcher is active, this method does nothing.
    /// This is an async call to use with [`crux_core::compose::Compose`].
    pub async fn clear_watch_async(&self) {
        let maybe_id = self.watch_id.lock().unwrap().take();
        if let Some(id) = maybe_id {
            self.context.notify_shell(Request::ClearWatch(id)).await
        }
    }
}
