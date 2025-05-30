//! Some types to store information to be viewed by the UI.
//!
//! The general theme is that these types formats numbers and units to strings so that the UI don't
//! have to bother with that.

use arrayvec::ArrayVec;
use chrono::{TimeDelta, prelude::*};
use compact_str::{CompactString, ToCompactString, format_compact};
use crux_geolocation::GeoInfo;
use itertools::Either;
use jord::{LatLong, spherical::Sphere};
use lazy_reaction::{DerivedSignal, Source};
use rstar::RTree;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

use super::geo_traits::*;
use super::{Event, InnerModel, PLANET, RecordedWay, SavedPos, rtree_point};

/// Precition for latitude and longitude.
const COORD_PRECITION: usize = 5;
/// Precition for altitude, volocity and other things.
const PRECITION: usize = 1;

/// Format latitude, longitude, altitude and accuracy.
fn format_pos(pos: &(impl Coords + Altitude)) -> ArrayVec<CompactString, 5> {
    let latitude = pos.coords().latitude().as_degrees();
    let longitude = pos.coords().longitude().as_degrees();
    let north_south = if latitude >= 0.0 { "North" } else { "South" };
    let east_west = if longitude >= 0.0 { "East" } else { "West" };
    let mut properties = ArrayVec::new();
    properties.push(format_compact!(
        "Latitude: {:.*}° {}",
        COORD_PRECITION,
        latitude,
        north_south
    ));
    properties.push(format_compact!(
        "Longitude: {:.*}° {}",
        COORD_PRECITION,
        longitude,
        east_west,
    ));
    if let Some(altitude) = pos.altitude() {
        properties.push(format_compact!(
            "Altitude: {:.*} meters",
            PRECITION,
            altitude.as_metres()
        ));
    }
    if let Some(accuracy) = pos.accuracy() {
        properties.push(format_compact!(
            "Accuracy: {} meters",
            accuracy.as_metres().round()
        ));
    }
    if let Some(altitude_accuracy) = pos.altitude_accuracy() {
        properties.push(format_compact!(
            "Altitude accuracy: {} meters",
            altitude_accuracy.as_metres().round()
        ));
    }
    properties
}

/// Format a timestamp.
fn format_timestamp(timestamp: DateTime<Utc>) -> CompactString {
    timestamp
        .with_timezone(&Local)
        .format("%a %b %e %T %Y")
        .to_compact_string()
}

/// Select the saved positions to view.
fn view_saved_positions_fn(
    saved_positions: Arc<RTree<SavedPos>>,
    n: usize,
    curr_pos: Option<LatLong>,
) -> Vec<ViewSavedPos> {
    if let Some(curr_pos) = curr_pos {
        Either::Left(saved_positions.nearest_neighbor_iter(&rtree_point(&curr_pos)))
    } else {
        Either::Right(saved_positions.iter())
    }
    .take(n)
    .map(|p| ViewSavedPos::new(p, curr_pos, true))
    .collect()
}

/// A trait for things which consists of a short summary, some properties, and maybe even some more
/// properties.
pub trait ViewObject {
    fn summary(&self) -> &CompactString;
    fn properties(&self) -> &[CompactString];
    /// An event to delete the object.
    fn delete(&self) -> Option<Event>;
    /// Even more properties which are usually not very interesting. May be empty.
    fn more_properties(&self) -> &[CompactString] {
        &[]
    }
}

/// Information about a saved position.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
pub struct ViewSavedPos {
    /// The name of the saved position.
    pub name: CompactString,
    /// The name and (if it does exist) a distance and direction.
    pub summary: CompactString,
    /// A number of properties, like latitude and timestamp.
    pub properties: ArrayVec<CompactString, 6>,
    /// Whether it can be deleted.
    pub deleateable: bool,
}

impl ViewSavedPos {
    fn new(saved_pos: &SavedPos, curr_pos: Option<LatLong>, deleateable: bool) -> Self {
        let summary = if let Some(curr_coords) = curr_pos {
            format_compact!(
                "{}: {} m, {}°",
                saved_pos.name,
                PLANET
                    .distance(curr_coords.to_nvector(), saved_pos.pos.coords.to_nvector())
                    .as_metres()
                    .round(),
                Sphere::initial_bearing(
                    curr_coords.to_nvector(),
                    saved_pos.pos.coords.to_nvector()
                )
                .as_degrees()
                .round()
            )
        } else {
            saved_pos.name.clone()
        };

        let mut properties = ArrayVec::new();
        properties.extend(format_pos(saved_pos));
        properties.push(format_compact!(
            "Saved at: {}",
            format_timestamp(saved_pos.timestamp)
        ));
        Self {
            name: saved_pos.name.clone(),
            summary,
            properties,
            deleateable,
        }
    }
}

impl ViewObject for ViewSavedPos {
    fn summary(&self) -> &CompactString {
        &self.summary
    }
    fn properties(&self) -> &[CompactString] {
        &self.properties
    }
    fn delete(&self) -> Option<Event> {
        if self.deleateable {
            Some(Event::DelSavedPos(self.name.clone()))
        } else {
            None
        }
    }
}

/// Information about speed and bearing.
fn format_speed_and_heading(geo: &GeoInfo) -> ArrayVec<CompactString, 2> {
    let mut properties = ArrayVec::new();
    if let Some(speed) = geo.volocity {
        properties.push(format_compact!(
            "Speed: {:.*} m/s",
            PRECITION,
            speed.as_metres_per_second()
        ));
    }
    if let Some(heading) = geo.bearing {
        properties.push(format_compact!("Heading {}°", heading.as_degrees().round()));
    }
    properties
}

/// Information about a way which is being recorded.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
pub struct ViewRecordedWay {
    /// The name of the recorded way.
    pub name: CompactString,
    /// The elapsed time, distance and average speed.
    pub summary: CompactString,
    /// A number of properties, like number of nodes.
    pub properties: ArrayVec<CompactString, 3>,
    pub deleateable: bool,
}

impl ViewRecordedWay {
    pub(crate) fn new(name: impl fmt::Display, rec: &RecordedWay, deleateable: bool) -> Self {
        let summary = format_compact!("{}: {} meters", name, rec.way.length().as_metres().round());
        let properties = if rec.way.nodes().len() > 0 {
            ArrayVec::from([
                format_compact!("Number of nodes: {}", rec.way.nodes().len()),
                format_compact!(
                    "Start time: {}",
                    format_timestamp(rec.way().nodes().first().unwrap().timestamp())
                ),
                format_compact!(
                    "End time: {}",
                    format_timestamp(rec.way().nodes().last().unwrap().timestamp())
                ),
            ])
        } else {
            let mut p = ArrayVec::new();
            p.push("The way doesn't have any nodes.".to_compact_string());
            p
        };
        Self {
            name: name.to_compact_string(),
            summary,
            properties,
            deleateable,
        }
    }
}

impl ViewObject for ViewRecordedWay {
    fn summary(&self) -> &CompactString {
        &self.summary
    }
    fn properties(&self) -> &[CompactString] {
        &self.properties
    }
    fn delete(&self) -> Option<Event> {
        if self.deleateable {
            Some(Event::DelRecordedWay(self.name.clone()))
        } else {
            None
        }
    }
}

/// The entire view model. This is everything sent to the UI.
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
pub struct ViewModel {
    /// Information about the GPS status. May display an error, especially if current_pos is
    /// `None`. Otherwise it should display accuracy and such.
    pub gps_status: CompactString,

    /// Properties like latitude and volocity about the current position. May be empty.
    pub curr_pos_properties: Arc<ArrayVec<CompactString, 7>>,
    /// The list of saved positions that the user wants to show. Might be empty.
    pub saved_positions: Arc<Vec<ViewSavedPos>>,
    /// The recorded way containing all positions since the app started.
    ///
    /// Updated very frequently -- at every position update.
    pub way_since_app_start: Arc<Option<ViewRecordedWay>>,
    /// List of saved recorded ways to show. Might be empty if the user doesn't want to show
    /// anything.
    pub recorded_ways: Arc<Vec<ViewRecordedWay>>,
    /// A message that should be displayed to the user.
    pub msg: Option<CompactString>,
}

impl ViewModel {
    pub(super) fn make(model: &InnerModel) -> DerivedSignal<Arc<Self>> {
        // Count the number of positions received in the last minute.
        let positions_in_last_minute = model.rgraph.memo(
            (model.all_positions.subscribe(), model.curr_time.subscribe()),
            |(all_positions, curr_time)| {
                (*all_positions)
                    .as_ref()
                    .and_then(|rec| {
                        curr_time
                            .as_ref()
                            .map(|t| rec.get_since(*t - TimeDelta::minutes(1)).len())
                    })
                    .unwrap_or(0)
            },
        );

        // Create a string showing some general geo status.
        let geo_status = model.rgraph.memo(
            (model.curr_pos.subscribe(), positions_in_last_minute),
            |(curr_pos, positions_in_last_minute)| match curr_pos {
                None => "No GPS information".into(),
                Some(Err(e)) => format_compact!("GPS Error: {}", e),
                Some(Ok(GeoInfo {
                    accuracy,
                    altitude_accuracy,
                    ..
                })) => {
                    let mut text = CompactString::new("");
                    if let Some(a) = accuracy {
                        text += &format_compact!("Accuracy: {:.*} m, ", PRECITION, a.as_metres());
                    }
                    if let Some(aa) = altitude_accuracy {
                        text += &format_compact!(
                            "Altitude accuracy: {:.*} m, ",
                            PRECITION,
                            aa.as_metres()
                        );
                    }
                    text += &format_compact!(
                        "{} positions in the last minute.",
                        positions_in_last_minute
                    );
                    text
                }
            },
        );

        // Collect the n nearest saved positions.
        let saved_positions = model.rgraph.memo(
            (
                model.saved_positions.subscribe(),
                model.view_n_saved_positions.subscribe(),
                model
                    .curr_pos
                    .subscribe()
                    .map(|x| x.and_then(|x| x.ok().map(|x| x.coords))),
            ),
            |(saved_positions, n, curr_pos)| {
                Arc::new(view_saved_positions_fn(saved_positions, n, curr_pos))
            },
        );

        // Write some properties about the current position.
        let curr_pos_properties = model.rgraph.memo(model.curr_pos.subscribe(), |curr_pos| {
            let mut curr_pos_properties = ArrayVec::new();
            if let Some(p) = curr_pos.as_ref().map(|x| x.as_ref().ok()).flatten() {
                curr_pos_properties.extend(format_speed_and_heading(p));
                curr_pos_properties.extend(format_pos(p));
            }
            Arc::new(curr_pos_properties)
        });

        // When we list recorded ways we first list the way visiting all nodes since the app was
        // started. This will be updated very frequently so we don't want it to trigger a rerender
        // of the other recorded ways, so we handle it separately.
        let way_since_app_start =
            model
                .rgraph
                .derived_signal(model.all_positions.subscribe(), |all_positions| {
                    Arc::new(
                        (*all_positions)
                            .as_ref()
                            .map(|x| ViewRecordedWay::new("Since app start", x, false)),
                    )
                });

        // Collect n recorded ways that the user want to show.
        //
        // TODO: It should probably be the n most relevant or nearest or something, ways, now it is
        // just n arbitrary ways which is not so good.
        let recorded_ways = model.rgraph.derived_signal(
            (
                model.recorded_ways.subscribe(),
                model.view_n_recorded_ways.subscribe(),
            ),
            |(saved_recorded_ways, n)| {
                Arc::new(
                    saved_recorded_ways
                        .iter()
                        .map(move |(name, way)| ViewRecordedWay::new(name, way, true))
                        .take(n.saturating_sub(1))
                        .collect::<Vec<_>>(),
                )
            },
        );

        model.rgraph.derived_signal(
            (
                geo_status,
                curr_pos_properties,
                saved_positions,
                way_since_app_start,
                recorded_ways,
                model.msg.subscribe(),
            ),
            |(
                gps_status,
                curr_pos_properties,
                saved_positions,
                way_since_app_start,
                recorded_ways,
                msg,
            )| {
                Arc::new(Self {
                    gps_status,
                    curr_pos_properties,
                    saved_positions,
                    way_since_app_start,
                    recorded_ways,
                    msg: if msg.is_empty() { None } else { Some(msg) },
                })
            },
        )
    }
}
