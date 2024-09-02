#![allow(unused_imports)]
mod core;

use core::App;
use std::fmt;
use std::hash::{DefaultHasher, Hash};
use std::rc::Rc;

use build_time::build_time_local;
use leptos::{
    component, create_effect, create_node_ref, ev, event_target, html, signal_prelude::*, web_sys,
    IntoView,
};
use shared::{view_types::ViewModel, Event};

#[component]
fn RootComponent() -> impl IntoView {
    let app = App::new();
    html::div().child((
        curr_pos_component(app),
        list_saved_positions(app),
        save_pos_component(app),
        show_msg_component(app),
        footer_component(),
    ))
}

fn curr_pos_component(app: App) -> impl IntoView {
    let view = move || app.view.get();
    html::div().child((
        html::h3().child("Current Position"),
        html::p().child((
            ("Status: ", move || view().gps_status.to_string()),
            move || {
                view().curr_pos.as_ref().map(|pos| {
                    (
                        html::br(),
                        format!("{}", pos.latitude),
                        html::br(),
                        format!("{}", pos.longitude),
                        pos.altitude
                            .as_ref()
                            .map(|x| (html::br(), "Altitude: ", format!("{x}"))),
                    )
                })
            },
        )),
    ))
}

fn list_saved_positions(app: App) -> impl IntoView {
    let no_saved_positions = create_memo(move |_| app.view.get().saved_positions.len());
    html::details()
        .on(ev::toggle, move |ev| {
            let is_open = event_target::<web_sys::HtmlDetailsElement>(&ev).open();
            app.set_event
                .set(Event::ViewNSavedPositions(if is_open { 10 } else { 0 }));
        })
        .child((
            html::summary().child("Nearest Saved positions"),
            move || {
                (0..no_saved_positions.get())
                    .into_iter()
                    .map(|i| {
                        html::details().child(move || {
                            let pos = &app.view.get().saved_positions[i];
                            (
                                html::summary().child(pos.summary.to_string()),
                                pos.properties
                                    .iter()
                                    .map(|x| (x.to_string(), html::br()))
                                    .collect::<Vec<_>>(),
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            },
        ))
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

fn footer_component() -> impl IntoView {
    html::footer().child(html::p().child(("Built at ", build_time_local!("%Y-%m-%d %H:%M:%S %Z."))))
}

fn main() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    leptos::mount_to_body(|| RootComponent());
}
