#![allow(unused_variables, dead_code)]
mod geolocation;
mod storage;
use std::cell::RefCell;
use std::rc::Rc;

use chrono::Utc;
use crux_geolocation::{GeoOperation, GeoOptions};
use crux_kv::{KeyValueOperation, KeyValueResponse, KeyValueResult, value::Value};
use crux_time::{TimeRequest, TimeResponse};
use leptos::signal_prelude::*;
use leptos::watch;
use shared::{Effect, Event, FileDownloadOperation, GeoApp, Request, view_types::ViewModel};

/// Signals to send events to and get the last view model from the app.
#[derive(Clone, Copy)]
pub struct App {
    /// Signal to receive the latest view model.
    pub view: ReadSignal<Rc<ViewModel>>,
    /// Signal to send events to the app.
    pub set_event: WriteSignal<Event>,
    /// Signal to receive a `FileDownloadRequest`.
    pub file_download: RwSignal<Option<FileDownloadOperation>>,
}

/// A backend struct for the app.
struct Backend {
    /// The core of the app.
    core: shared::Core<GeoApp>,
    /// Signal where new view models are sent from the core.
    render: WriteSignal<Rc<ViewModel>>,
    /// Signal to receive events that should be sent to the core.
    event: ReadSignal<Event>,
    /// Signal to set a file download request.
    set_file_download: WriteSignal<Option<FileDownloadOperation>>,
    /// A possible current watch on the geolocation API.
    geo_watch: WriteSignal<geolocation::Event>,
}

impl App {
    pub fn new() -> Self {
        let core = shared::Core::new();
        let (view, render) = create_signal(Rc::new(core.view()));
        let (event, set_event) = create_signal(Event::StartGeolocation);
        let file_download = create_rw_signal(None);
        let backend = Rc::new(Backend {
            core,
            render,
            event,
            set_file_download: file_download.write_only(),
            geo_watch: geolocation::create_geo_watch(),
        });
        let _ = watch(
            move || event.get(),
            move |event, _, _| {
                let effects = backend.core.process_event(event.clone());
                backend.process_effects(effects);
            },
            true,
        );
        set_event.set(Event::LoadPersistantData);
        Self {
            view,
            set_event,
            file_download,
        }
    }
}

impl Backend {
    /// Process a bunch of effects from the core.
    pub fn process_effects(self: &Rc<Self>, effects: impl IntoIterator<Item = Effect>) {
        for effect in effects {
            match effect {
                Effect::Render(_) => {
                    self.render.set(Rc::new(self.core.view()));
                }
                Effect::Time(req) => self.clone().process_time(req),
                Effect::Storage(req) => self.process_storage(req),
                Effect::Geolocation(req) => self.process_geolocation(req),
                Effect::FileDownload(req) => self.set_file_download.set(Some(req.operation)),
            }
        }
    }

    /// Process a time request from the core.
    fn process_time(self: Rc<Self>, mut request: Request<TimeRequest>) {
        match request.operation {
            TimeRequest::Now => {
                let response = TimeResponse::Now {
                    instant: Utc::now().try_into().unwrap(),
                };
                self.process_effects(self.core.resolve(&mut request, response).unwrap());
            }
            TimeRequest::NotifyAfter { duration, id } => leptos::set_timeout(
                move || {
                    self.process_effects(
                        self.core
                            .resolve(&mut request, TimeResponse::DurationElapsed { id })
                            .unwrap(),
                    )
                },
                TryInto::<chrono::TimeDelta>::try_into(duration)
                    .unwrap()
                    .to_std()
                    .unwrap(),
            ),
            TimeRequest::NotifyAt { instant, id } => leptos::set_timeout(
                move || {
                    self.process_effects(
                        self.core
                            .resolve(&mut request, TimeResponse::InstantArrived { id })
                            .unwrap(),
                    )
                },
                (TryInto::<chrono::DateTime<Utc>>::try_into(instant).unwrap() - Utc::now())
                    .to_std()
                    .unwrap_or(std::time::Duration::ZERO),
            ),
            TimeRequest::Clear { .. } => panic!("Operation not supported: TimeRequest::Clear"),
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
                self.process_effects(self.core.resolve(&mut request, response).unwrap());
            }
            KeyValueOperation::Set { key, value } => {
                storage::set(key, value);
                let response = KeyValueResult::Ok {
                    response: KeyValueResponse::Set {
                        previous: Value::None,
                    },
                };
                self.process_effects(self.core.resolve(&mut request, response).unwrap());
            }
            KeyValueOperation::Delete { key } => {
                storage::delete(key);
                let response = KeyValueResult::Ok {
                    response: KeyValueResponse::Delete {
                        previous: Value::None,
                    },
                };
                self.process_effects(self.core.resolve(&mut request, response).unwrap());
            }
            KeyValueOperation::Exists { key } => {
                let is_present = storage::get(key).is_some();
                let response = KeyValueResult::Ok {
                    response: KeyValueResponse::Exists { is_present },
                };
                self.process_effects(self.core.resolve(&mut request, response).unwrap());
            }
            KeyValueOperation::ListKeys { .. } => unimplemented!(),
        }
    }

    /// Process a geolocation request from the core.
    fn process_geolocation(self: &Rc<Self>, req: Request<GeoOperation>) {
        match req.operation {
            GeoOperation::WatchPosition(opts) => self.geo_watch.set(geolocation::Event::Watch {
                backend: self.clone(),
                req: Rc::new(RefCell::new(req)),
                opts,
            }),
            GeoOperation::ClearWatch => self.geo_watch.set(geolocation::Event::Stop),
        }
    }
}
