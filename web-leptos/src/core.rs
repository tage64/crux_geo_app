#![allow(unused_variables, dead_code)]
mod geolocation;
mod storage;
use std::cell::RefCell;
use std::rc::Rc;

use chrono::Utc;
use crux_geolocation::{GeoOptions, GeoRequest};
use crux_kv::{value::Value, KeyValueOperation, KeyValueResponse, KeyValueResult};
use crux_time::{TimeRequest, TimeResponse};
use geolocation::GeoWatch;
use leptos::signal_prelude::*;
use leptos::watch;
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
        let _ = watch(
            move || event.get(),
            move |event, _, _| {
                let effects = backend.core.process_event(event.clone());
                backend.process_effects(effects);
                // For some very strange reason, the geolocation service stops after this, so we
                // need to restart it.
                if backend.geo_watch.borrow().is_some() {
                    let effects = backend.core.process_event(Event::StartGeolocation);
                    backend.process_effects(effects);
                }
            },
            true,
        );
        set_event.set(Event::LoadSavedPositions);
        Self { view, set_event }
    }
}

impl Backend {
    /// Process a bunch of effects from the core.
    pub fn process_effects(self: &Rc<Self>, effects: impl IntoIterator<Item = Effect>) {
        for effect in effects {
            match effect {
                Effect::Render(_) => {
                    self.render.set(self.core.view());
                }
                Effect::Time(req) => self.clone().process_time(req),
                Effect::KeyValue(req) => self.process_storage(req),
                Effect::Geolocation(req) => self.process_geolocation(req),
            }
        }
    }

    /// Process a time request from the core.
    fn process_time(self: Rc<Self>, mut request: Request<TimeRequest>) {
        match request.operation {
            TimeRequest::Now => {
                let response = TimeResponse::Now(Utc::now().try_into().unwrap());
                self.process_effects(self.core.resolve(&mut request, response));
            }
            TimeRequest::NotifyAfter(duration) => leptos::set_timeout(
                move || {
                    self.process_effects(
                        self.core
                            .resolve(&mut request, TimeResponse::DurationElapsed),
                    )
                },
                TryInto::<chrono::TimeDelta>::try_into(duration)
                    .unwrap()
                    .to_std()
                    .unwrap(),
            ),
            TimeRequest::NotifyAt(duration) => leptos::set_timeout(
                move || {
                    self.process_effects(
                        self.core
                            .resolve(&mut request, TimeResponse::DurationElapsed),
                    )
                },
                (TryInto::<chrono::DateTime<Utc>>::try_into(duration).unwrap() - Utc::now())
                    .to_std()
                    .unwrap_or(std::time::Duration::ZERO),
            ),
        }
    }

    /// Handle persistant storage operations.
    fn process_storage(self: &Rc<Self>, mut request: Request<KeyValueOperation>) {
        match request.operation.clone() {
            KeyValueOperation::Get { key } => {
                let val = storage::get(key);
                let value = val.map(Value::Bytes).unwrap_or(Value::None);
                let response = KeyValueResult::Ok {
                    response: KeyValueResponse::Get { value },
                };
                self.process_effects(self.core.resolve(&mut request, response));
            }
            KeyValueOperation::Set { key, value } => {
                storage::set(key, value);
                let response = KeyValueResult::Ok {
                    response: KeyValueResponse::Set {
                        previous: Value::None,
                    },
                };
                self.process_effects(self.core.resolve(&mut request, response));
            }
            KeyValueOperation::Delete { key } => {
                storage::delete(key);
                let response = KeyValueResult::Ok {
                    response: KeyValueResponse::Delete {
                        previous: Value::None,
                    },
                };
                self.process_effects(self.core.resolve(&mut request, response));
            }
            KeyValueOperation::Exists { key } => {
                let is_present = storage::get(key).is_some();
                let response = KeyValueResult::Ok {
                    response: KeyValueResponse::Exists { is_present },
                };
                self.process_effects(self.core.resolve(&mut request, response));
            }
            KeyValueOperation::ListKeys { .. } => unimplemented!(),
        }
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
