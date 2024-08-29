#![allow(unused_variables, dead_code)]
use std::rc::Rc;

use crux_geolocation::{GeoOptions, GeoRequest};
use leptos::{SignalUpdate, WriteSignal};
use leptos_use::{use_geolocation_with_options, UseGeolocationOptions, UseGeolocationReturn};
use shared::{view_types::ViewModel, Effect, Event, GeoApp, Request};

/// The app type that will be passed around.
pub type App = Rc<AppStruct>;

/// Core struct with information about the app.
pub struct AppStruct {
    /// The core of the app.
    pub core: shared::Core<Effect, GeoApp>,
}

pub fn new_app() -> App {
    Rc::new(AppStruct {
        core: shared::Core::new(),
    })
}

pub fn update(app: &App, event: Event, render: WriteSignal<ViewModel>) {
    for effect in app.core.process_event(event) {
        process_effect(app, effect, render);
    }
}

pub fn process_effect(app: &App, effect: Effect, render: WriteSignal<ViewModel>) {
    match effect {
        Effect::Render(_) => {
            render.update(|view| *view = app.core.view());
        }
        Effect::Geolocation(req) => process_geolocation(app, req),
    };
}

fn process_geolocation(app: &App, request: Request<GeoRequest>) {
    match &request.operation {
        GeoRequest::GetCurrentPosition(opts) => {
            todo!();
        }
        GeoRequest::WatchPosition(opts) => {
            todo!();
        }
        GeoRequest::ClearWatch(_) => {
            todo!();
        }
    }
}

/// Convert a `GeoOptions` struct from `crux_geolocation` to a similar "options struct" used by
/// `leptos_use`.
fn convert_geo_options(opts: GeoOptions) -> UseGeolocationOptions {
    UseGeolocationOptions::default()
        .immediate(false)
        .enable_high_accuracy(opts.enable_high_accuracy)
        .maximum_age(opts.maximum_age.try_into().unwrap_or(u32::MAX))
        .timeout(
            opts.timeout
                .unwrap_or(u64::MAX)
                .try_into()
                .unwrap_or(u32::MAX),
        )
}
