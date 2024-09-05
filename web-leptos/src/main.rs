#![allow(unused_imports)]
mod core;

use core::App;
use std::fmt;
use std::hash::{DefaultHasher, Hash};
use std::rc::Rc;

use base64::prelude::*;
use build_time::build_time_local;
use leptos::{
    component, create_effect, create_node_ref, ev, event_target, html, logging::warn,
    signal_prelude::*, web_sys, IntoView,
};
use shared::{
    view_types::{ViewModel, ViewObject},
    Event,
};

#[component]
fn RootComponent() -> impl IntoView {
    let app = App::new();
    html::div().child((
        curr_pos_component(app),
        list_items(
            app,
            "Nearest saved positions",
            Event::ViewNSavedPositions,
            |v| &v.saved_positions,
        ),
        save_pos_component(app),
        list_items(app, "Recorded ways", Event::ViewNRecordedWays, |v| {
            &v.recorded_ways
        }),
        save_way_component(app),
        show_msg_component(app),
        file_download_component(app),
        footer_component(),
    ))
}

fn curr_pos_component(app: App) -> impl IntoView {
    let body = move || {
        let view = app.view.get();
        ("Status: ", view.gps_status.to_string(), move || {
            view.curr_pos_properties
                .iter()
                .map(|x| (html::br(), x.to_string()))
                .collect::<Vec<_>>()
        })
    };
    html::section().child((html::h3().child("Current Position"), html::p().child(body)))
}

fn list_items<T: ViewObject>(
    app: App,
    summary: &'static str,
    view_n_event: impl Fn(usize) -> Event + 'static,
    items: impl Fn(&ViewModel) -> &[T] + Copy + 'static,
) -> impl IntoView {
    // Number of things.
    let no_items = create_memo(move |_| items(&app.view.get()).len());
    let body = html::details()
        .on(ev::toggle, move |ev| {
            let is_open = event_target::<web_sys::HtmlDetailsElement>(&ev).open();
            app.set_event
                .set(view_n_event(if is_open { 10 } else { 0 }));
        })
        .child((html::summary().child(summary), move || {
            (0..no_items.get())
                .into_iter()
                .map(move |i| {
                    html::details().child(move || {
                        let view = app.view.get();
                        let item = &items(&view)[i];
                        (
                            html::summary().child(item.summary().to_string()),
                            item.properties()
                                .iter()
                                .map(|x| (x.to_string(), html::br()))
                                .collect::<Vec<_>>(),
                        )
                    })
                })
                .collect::<Vec<_>>()
        }));
    html::p().child(body)
}

fn save_pos_component(app: App) -> impl IntoView {
    let (save_pos_dialog, set_save_pos_dialog) = create_signal(false);
    let input_node = create_node_ref();
    move || {
        if save_pos_dialog.get() {
            html::form()
                .child(
                    html::label()
                        .attr("for", "name")
                        .child("Name of the position"),
                )
                .child(
                    html::input()
                        .attr("type", "text")
                        .attr("name", "name")
                        .attr("autofocus", true)
                        .node_ref(input_node),
                )
                .child(html::input().attr("type", "submit").attr("value", "Submit"))
                .on(ev::submit, move |event| {
                    event.prevent_default();
                    let name = input_node
                        .get()
                        .expect("Input element should be initialized.")
                        .value();
                    app.set_event.set(Event::SaveCurrPos(name.into()));
                    set_save_pos_dialog.set(false);
                })
                .into_any()
        } else {
            html::button()
                .on(ev::click, move |_| set_save_pos_dialog.set(true))
                .child("Save the Current Position ")
                .into_any()
        }
    }
}

fn save_way_component(app: App) -> impl IntoView {
    let (save_way_dialog, set_save_way_dialog) = create_signal(false);
    let input_node = create_node_ref();
    move || {
        if save_way_dialog.get() {
            html::form()
                .child(html::label().attr("for", "name").child("Name of the way"))
                .child(
                    html::input()
                        .attr("type", "text")
                        .attr("name", "name")
                        .attr("autofocus", true)
                        .node_ref(input_node),
                )
                .child(html::input().attr("type", "submit").attr("value", "Submit"))
                .on(ev::submit, move |event| {
                    event.prevent_default();
                    let name = input_node
                        .get()
                        .expect("Input element should be initialized.")
                        .value();
                    app.set_event.set(Event::SaveAllPositions(name.into()));
                    set_save_way_dialog.set(false);
                })
                .into_any()
        } else {
            html::button()
                .on(ev::click, move |_| set_save_way_dialog.set(true))
                .child("Save the Current Way ")
                .into_any()
        }
    }
}

fn show_msg_component(app: App) -> impl IntoView {
    html::div().child((
        html::hr(),
        html::p().attr("role", "alert").child(move || {
            let view = app.view.get();
            if let Some(msg) = &view.msg {
                msg.to_string()
            } else {
                String::new()
            }
        }),
    ))
}

fn file_download_component(app: App) -> impl IntoView {
    move || {
        let f = app.file_download.get();
        if let Some(f) = f {
            let content_len = f.content.len();
            let download_link = html::a()
                .attr("download", f.file_name.unwrap_or_default().to_string())
                .attr(
                    "href",
                    format!(
                        "data:{};base64,{}",
                        f.mime_type.unwrap_or_default(),
                        BASE64_STANDARD.encode(f.content)
                    ),
                )
                .on(ev::click, move |_| app.file_download.set(None))
                .attr("autofocus", true)
                .child(format!(
                    "Download JSON file ({:.2} kb)",
                    content_len as f32 / 1000.0
                ));
            let cancel_button = html::button()
                .on(ev::click, move |_| app.file_download.set(None))
                .child("Cancel");
            html::p().child((download_link, cancel_button)).into_any()
        } else {
            html::button()
                .on(ev::click, move |_| app.set_event.set(Event::DownloadData))
                .child("Download all Saved Data as JSON")
                .into_any()
        }
    }
}

fn footer_component() -> impl IntoView {
    html::footer().child(html::p().child(("Built at ", build_time_local!("%Y-%m-%d %H:%M:%S %Z."))))
}

fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    leptos::mount_to_body(|| RootComponent());
}
