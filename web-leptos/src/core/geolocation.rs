//! Interface to the
//! [Geolocation Web API](https://developer.mozilla.org/en-US/docs/Web/API/Geolocation_API)
//! using leptos_use.
use std::cell::RefCell;
use std::rc::Rc;

use crux_geolocation::{GeoOptions, GeoRequest, GeoResponse, Position};
use leptos::signal_prelude::*;
use leptos::{create_effect, web_sys, Effect};
use leptos_use::{use_geolocation_with_options, UseGeolocationOptions, UseGeolocationReturn};
use shared::{Event, Request};

use super::Backend;

/// A handle to watch the current position.
pub struct GeoWatch {
    /// Function to stop the watch.
    stop_fn: Box<dyn FnOnce()>,
    effect: Effect<()>,
}

impl GeoWatch {
    /// Begin watching the position.
    pub fn watch(backend: Rc<Backend>, request: Request<GeoRequest>, opts: GeoOptions) -> Self {
        let UseGeolocationReturn {
            coords: get_coords,
            located_at: get_timestamp,
            error: get_error,
            pause: stop_fn,
            resume: _,
        } = use_geolocation_with_options(convert_geo_options(opts));

        let request = RefCell::new(request);
        let effect = create_effect(move |_| {
            let coords = get_coords.get();
            let timestamp = get_timestamp.get();
            let error = get_error.get();
            let geo_response = if let Some(err) = error {
                convert_error(err)
            } else {
                let (Some(coords), Some(timestamp)) = (coords, timestamp) else {
                    return;
                };
                convert_position(coords, timestamp)
            };
            let effects = backend
                .core
                .resolve(&mut request.borrow_mut(), geo_response);
            backend.process_effects(effects);
        });

        Self {
            stop_fn: Box::new(stop_fn),
            effect,
        }
    }

    /// Stop the watching of position.
    pub fn stop_watch(self) {
        (self.stop_fn)();
        self.effect.dispose();
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
fn convert_error(err: web_sys::PositionError) -> GeoResponse {
    use web_sys::PositionError;
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
