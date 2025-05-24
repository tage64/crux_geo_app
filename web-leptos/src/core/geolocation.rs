//! Interface to the
//! [Geolocation Web API](https://developer.mozilla.org/en-US/docs/Web/API/Geolocation_API)
//! using leptos_use.
use std::cmp;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crux_geolocation::{GeoOperation, GeoOptions, GeoResponse, Position};
use leptos::prelude::*;
use leptos::web_sys::{self, PositionError};
use leptos_use::{UseGeolocationOptions, UseGeolocationReturn, use_geolocation_with_options};
use shared::Request;

use super::Backend;

/// Event for a geo_watch
#[derive(Clone)]
pub enum Event {
    Watch {
        backend: Arc<Backend>,
        /// Wrapped in Arc to facilitate cloning.
        req: Arc<Mutex<Request<GeoOperation>>>,
        opts: GeoOptions,
    },
    Stop,
}

/// A state for the geo watcher.
enum GeoWatch {
    /// No watch at the moment.
    Idle,
    /// A living watch.
    Alive {
        /// Function to stop the watch.
        stop_fn: Box<dyn Fn()>,
    },
    /// The watch is retrying with a timeout.
    Retry { n: u64, handle: TimeoutHandle },
}

pub fn create_geo_watch() -> WriteSignal<Event> {
    let (get_event, set_event) = signal(Event::Stop);
    let geo_watch = Arc::new(Mutex::new(GeoWatch::Idle));
    Effect::new(move |_| match get_event.get() {
        Event::Stop => geo_watch.lock().unwrap().stop(),
        Event::Watch { backend, req, opts } => {
            GeoWatch::watch(geo_watch.clone(), set_event, backend, req, opts);
        }
    });
    set_event
}

impl GeoWatch {
    /// Stop a watch. (Idempotent)
    fn stop(&mut self) {
        match self {
            Self::Idle => (),
            Self::Alive { stop_fn } => stop_fn(),
            Self::Retry { handle, .. } => handle.clear(),
        }
        *self = Self::Idle;
    }

    /// Begin watching the position.
    pub fn watch(
        self_: Arc<Mutex<Self>>,
        set_event: WriteSignal<Event>,
        backend: Arc<Backend>,
        request: Arc<Mutex<Request<GeoOperation>>>,
        opts: GeoOptions,
    ) {
        let mut self_guard = self_.lock().unwrap();
        let n_retries = if let Self::Retry { n, .. } = *self_guard {
            n
        } else {
            0
        };
        self_guard.stop();
        let UseGeolocationReturn {
            coords: get_coords,
            located_at: get_timestamp,
            error: get_error,
            pause: stop_fn,
            resume: _,
        } = use_geolocation_with_options(convert_geo_options(opts));
        *self_guard = Self::Alive {
            stop_fn: Box::new(stop_fn),
        };
        drop(self_guard);

        Effect::new(move |_| {
            let coords = get_coords.get();
            let timestamp = get_timestamp.get();
            let error = get_error.get();
            let geo_response = if let Some(err) = error {
                let backend = backend.clone();
                let req = request.clone();
                let handle = set_timeout_with_handle(
                    move || set_event.set(Event::Watch { backend, req, opts }),
                    retry_time(n_retries),
                )
                .unwrap();
                *self_.lock().unwrap() = Self::Retry {
                    n: n_retries + 1,
                    handle,
                };
                convert_error(err)
            } else {
                let (Some(coords), Some(timestamp)) = (coords, timestamp) else {
                    return;
                };
                convert_position(coords, timestamp)
            };
            let effects = backend
                .core
                .resolve(&mut *request.lock().unwrap(), geo_response)
                .unwrap();
            backend.process_effects(effects);
        });
    }
}

/// Convert a `GeoOptions` struct from `crux_geolocation` to a similar "options struct" used by
/// `leptos_use`.
fn convert_geo_options(opts: GeoOptions) -> UseGeolocationOptions {
    UseGeolocationOptions::default()
        .immediate(true)
        .enable_high_accuracy(opts.enable_high_accuracy)
        .maximum_age(opts.maximum_age.try_into().unwrap_or(u32::MAX))
        .timeout(
            opts.timeout
                .unwrap_or(u64::MAX)
                .try_into()
                .unwrap_or(u32::MAX),
        )
}

/// Convert a `web_sys::PositionError` to a `GeoResponse`.
fn convert_error(err: PositionError) -> GeoResponse {
    match err.code() {
        PositionError::PERMISSION_DENIED => GeoResponse::PermissionDeniedError,
        PositionError::POSITION_UNAVAILABLE => GeoResponse::PositionUnavailableError,
        PositionError::TIMEOUT => GeoResponse::TimeoutError,
        x => panic!("Unexpected error code from geolocation: {x}"),
    }
}

/// Convert a `web_sys::Coordinates` and a timestamp to a `GeoResponse`.
fn convert_position(coords: web_sys::Coordinates, timestamp: f64) -> GeoResponse {
    GeoResponse::Position {
        timestamp: timestamp.round() as i64,
        coords: Position {
            latitude: coords.latitude(),
            longitude: coords.longitude(),
            altitude: coords.altitude(),
            accuracy: Some(coords.accuracy()),
            altitude_accuracy: coords.altitude_accuracy(),
            heading: coords.heading(),
            volocity: coords.speed(),
        },
    }
}

/// Calculate retry time for the nth retry.
///
/// Contains lots of hard coded numbers.
fn retry_time(n: u64) -> Duration {
    if n == 0 {
        Duration::ZERO
    } else {
        cmp::min(
            Duration::from_millis(250 * 2u64.pow(n.try_into().unwrap_or(u32::MAX)) - 1),
            Duration::from_secs(10),
        )
    }
}
