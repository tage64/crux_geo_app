#![allow(unused_imports)]
mod core;

use core::App;
use std::fmt;
use std::hash::{DefaultHasher, Hash};

use leptos::{component, create_effect, create_node_ref, ev, html, signal_prelude::*, IntoView};
use shared::{view_types::ViewModel, Event};

#[component]
fn RootComponent() -> impl IntoView {
    let app = App::new();
    html::div().child((
        curr_pos_component(app),
        list_saved_positions(app),
        save_pos_component(app),
    ))
}

fn curr_pos_component(app: App) -> impl IntoView {
    html::div().child((
        html::h3().child("Current Position"),
        html::p().child(move || {
            let view = app.view.get();
            (
                format!("status: {}", view.gps_status),
                view.curr_pos.as_ref().map(|pos| {
                    (
                        html::br(),
                        format!("{}", pos.latitude),
                        html::br(),
                        format!("{}", pos.longitude),
                        pos.altitude.as_ref().map(|x| (html::br(), format!("{x}"))),
                    )
                }),
            )
        }),
    ))
}

fn list_saved_positions(app: App) -> impl IntoView {
    html::div().child((
        html::h3().child("Near Positions"),
        html::p().child(move || format!("{} saved positions", app.view.get().near_positions.len())),
        leptos::For(leptos::ForProps {
            each: move || app.view.get().near_positions,
            key: |x| x.hash(&mut DefaultHasher::new()),
            children: |x| {
                format!(
                    "{}: {}, {}, {}",
                    x.name, x.latitude, x.longitude, x.timestamp
                )
            },
        }),
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

fn main() {
    leptos::mount_to_body(|| RootComponent());
}
