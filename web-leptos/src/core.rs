#![allow(unused_variables, dead_code)]
mod geolocation;
use std::cell::RefCell;
use std::rc::Rc;

use crux_geolocation::{GeoOptions, GeoRequest};
use geolocation::GeoWatch;
use leptos::create_effect;
use leptos::signal_prelude::*;
use shared::{view_types::ViewModel, Effect, Event, GeoApp, Request};

/// Signals to send events to and get the last view model from the app.
#[derive(Clone, Copy)]
pub struct App {
    /// Signal to receive the latest view model.
    pub view: ReadSignal<ViewModel>,
    /// Signal to send events to the app.
    pub set_event: WriteSignal<Event>,
}

/// A backend struct for the app.
struct Backend {
    /// The core of the app.
    core: shared::Core<Effect, GeoApp>,
    /// Signal where new view models are sent from the core.
    render: WriteSignal<ViewModel>,
    /// Signal to receive events that should be sent to the core.
    event: ReadSignal<Event>,
    /// A possible current watch on the geolocation API.
    geo_watch: RefCell<Option<GeoWatch>>,
}

impl App {
    pub fn new() -> Self {
        let core = shared::Core::new();
        let (view, render) = create_signal(core.view());
        let (event, set_event) = create_signal(Event::StartGeolocation);
        let backend = Rc::new(Backend {
            core,
            render,
            event,
            geo_watch: RefCell::new(None),
        });
        create_effect(move |_| {
            for effect in backend.core.process_event(backend.event.get()) {
                backend.process_effect(effect);
            }
        });
        Self { view, set_event }
    }
}

impl Backend {
    /// Process an effect from the core.
    pub fn process_effect(self: &Rc<Self>, effect: Effect) {
        match effect {
            Effect::Render(_) => {
                self.render.set(self.core.view());
            }
            Effect::Geolocation(req) => self.process_geolocation(req),
        };
    }

    /// Process a geolocation request from the core.
    fn process_geolocation(self: &Rc<Self>, request: Request<GeoRequest>) {
        match request.operation {
            GeoRequest::WatchPosition(opts) => {
                let mut geo_watch = self.geo_watch.borrow_mut();
                if let Some(geo_watch) = geo_watch.take() {
                    geo_watch.stop_watch();
                }
                *geo_watch = Some(GeoWatch::watch(self.clone(), request, opts));
            }
            GeoRequest::ClearWatch => {
                if let Some(geo_watch) = self.geo_watch.borrow_mut().take() {
                    geo_watch.stop_watch();
                }
            }
        }
    }
}
