//! Real-time road traffic overlay backed by the Sytadin (DiRIF) diffusion feed.
//!
//! This module holds the framework-agnostic data layer:
//! - [`TrafficGeometry`] parses the static road-network geometry
//!   (`Segment.mif`/`.mid`, Lambert II étendu) once at startup and reprojects
//!   every vertex to WGS84.
//! - [`build_snapshot`] joins that geometry with the dynamic segment states
//!   (`segments_dyn.xml`) and events (`evenements.xml`) into a ready-to-serve
//!   [`TrafficSnapshot`].
//!
//! The HTTP handler, polling loop and shared state live in
//! [`crate::api::traffic`].
//!
//! Data © Ministère chargé des transports / DiRIF — Sytadin®, subject to
//! usage conditions. Only fetched/derived at runtime, never redistributed.

use std::collections::HashMap;
use std::path::Path;

use proj4rs::Proj;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use serde::Serialize;
use tracing::warn;
use utoipa::ToSchema;

/// EPSG:27572 — Lambert II étendu (NTF, Clarke 1880 IGN), the projection used
/// by the Sytadin MIF/MID geometry. `towgs84` gives a ~few-metre-accurate datum
/// shift to WGS84, well within map-overlay tolerance.
///
/// `+pm=paris` is deliberately **not** declared: proj4rs adds the prime-meridian
/// offset as if it were radians, which lands the result ~130° off. The offset is
/// applied by hand in [`project_to_wgs84`] instead.
const LAMBERT_2E: &str = "+proj=lcc +lat_1=46.8 +lat_0=46.8 +lon_0=0 +k_0=0.99987742 \
     +x_0=600000 +y_0=2200000 +a=6378249.2 +b=6356515 \
     +towgs84=-168,-60,320,0,0,0,0 +units=m +no_defs";

/// EPSG:4326 — WGS84 geographic (the projection Leaflet expects).
const WGS84: &str = "+proj=longlat +datum=WGS84 +no_defs";

/// Paris prime meridian (2.5969213 grad) in degrees, east of Greenwich.
///
/// Added after the datum shift rather than before it, which costs ~7 m of
/// eastward accuracy — an order of magnitude below the overlay's needs.
const PARIS_MERIDIAN_DEG: f64 = 2.337_229_167;

/// Decimal places kept on serialized coordinates, to trim the payload.
///
/// 4 decimals ≈ 11 m, against ~50 m per pixel at the map's default zoom and
/// ~1.5 m at its deepest: the rounding stays below a pixel everywhere the
/// overlay is drawn.
const COORD_DECIMALS: f64 = 1e4;

// ---------------------------------------------------------------------------
// Static geometry
// ---------------------------------------------------------------------------

/// Road-network segment geometry, keyed by Sytadin `ID_SEGMENT`, in WGS84.
/// Built once at startup and never mutated afterwards.
pub struct TrafficGeometry {
    /// `ID_SEGMENT` → polyline as `[[lat, lon], ...]`.
    segments: HashMap<u32, Vec<[f64; 2]>>,
}

impl TrafficGeometry {
    /// Parse `Segment.mif` + `Segment.mid` from `dir` and reproject to WGS84.
    ///
    /// The MIF holds graphic objects (one `pline` per segment); the MID holds
    /// the matching attribute rows (first column = `ID_SEGMENT`). MIF/MID pair
    /// row-by-row, so the k-th geometry corresponds to the k-th attribute row.
    pub fn load(dir: &Path) -> Result<Self, String> {
        let geoms = parse_mif_geometry(&dir.join("Segment.mif"))?;
        let ids = parse_mid_ids(&dir.join("Segment.mid"))?;
        if geoms.len() != ids.len() {
            warn!(
                "Sytadin geometry/attribute mismatch: {} plines vs {} rows",
                geoms.len(),
                ids.len()
            );
        }

        let from = Proj::from_proj_string(LAMBERT_2E).map_err(|e| format!("source proj: {e}"))?;
        let to = Proj::from_proj_string(WGS84).map_err(|e| format!("target proj: {e}"))?;

        let mut segments = HashMap::with_capacity(ids.len());
        for (geom, id) in geoms.iter().zip(ids.iter()) {
            let Some(id) = id else { continue };
            let coords: Vec<[f64; 2]> = geom
                .iter()
                .filter_map(|&[x, y]| project_to_wgs84(&from, &to, x, y))
                .collect();
            if coords.len() >= 2 {
                segments.insert(*id, coords);
            }
        }
        Ok(Self { segments })
    }

    /// Number of indexed segments.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Whether no segment geometry was loaded.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Polyline for a segment, if known.
    fn segment(&self, id: u32) -> Option<&[[f64; 2]]> {
        self.segments.get(&id).map(Vec::as_slice)
    }

    /// Every indexed polyline, keyed by `ID_SEGMENT` — served as-is by
    /// `GET /api/traffic/geometry`.
    pub fn polylines(&self) -> &HashMap<u32, Vec<[f64; 2]>> {
        &self.segments
    }
}

/// Reproject a Lambert II étendu `(x, y)` in metres to `[lat, lon]` in degrees.
fn project_to_wgs84(from: &Proj, to: &Proj, x: f64, y: f64) -> Option<[f64; 2]> {
    let mut point = (x, y, 0.0);
    proj4rs::transform::transform(from, to, &mut point).ok()?;
    // proj4rs returns geographic coordinates in radians, x=lon, y=lat. The
    // longitude is relative to the Paris meridian (see [`LAMBERT_2E`]).
    Some([
        round(point.1.to_degrees()),
        round(point.0.to_degrees() + PARIS_MERIDIAN_DEG),
    ])
}

/// Round a coordinate to [`COORD_DECIMALS`] precision.
fn round(v: f64) -> f64 {
    (v * COORD_DECIMALS).round() / COORD_DECIMALS
}

/// Parse the MIF graphic objects into a list of polylines (Lambert `[x, y]`),
/// preserving file order so they align with the MID attribute rows.
fn parse_mif_geometry(path: &Path) -> Result<Vec<Vec<[f64; 2]>>, String> {
    // The MIF header declares WindowsLatin1, but the geometry section is pure
    // ASCII, so a lossy decode is safe here.
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);

    let mut lines = text.lines();
    // Skip the header up to and including the "Data" marker.
    for line in lines.by_ref() {
        if line.trim().eq_ignore_ascii_case("data") {
            break;
        }
    }

    let mut geoms = Vec::new();
    while let Some(line) = lines.next() {
        let line = line.trim();
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("pline") {
            let n: usize = rest.trim().parse().unwrap_or(0);
            let mut pts = Vec::with_capacity(n);
            for _ in 0..n {
                if let Some(p) = lines.next().and_then(parse_coord_line) {
                    pts.push(p);
                }
            }
            geoms.push(pts);
        } else if lower.starts_with("line ") {
            // "Line x1 y1 x2 y2"
            let nums = parse_numbers(line);
            if nums.len() == 4 {
                geoms.push(vec![[nums[0], nums[1]], [nums[2], nums[3]]]);
            }
        }
        // Pen/Brush/Symbol/Region and blank lines are ignored.
    }
    Ok(geoms)
}

/// Parse a single "x y" coordinate line into `[x, y]`.
fn parse_coord_line(line: &str) -> Option<[f64; 2]> {
    let nums = parse_numbers(line);
    (nums.len() >= 2).then(|| [nums[0], nums[1]])
}

/// Extract whitespace-separated floats from a line (skips any leading keyword).
fn parse_numbers(line: &str) -> Vec<f64> {
    line.split_whitespace()
        .filter_map(|t| t.parse::<f64>().ok())
        .collect()
}

/// Parse the first column (`ID_SEGMENT`) of each MID row, preserving order.
/// Rows whose first field is not an integer yield `None` to keep alignment.
fn parse_mid_ids(path: &Path) -> Result<Vec<Option<u32>>, String> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .map_err(|e| format!("open {}: {e}", path.display()))?;

    let mut ids = Vec::new();
    for record in reader.byte_records() {
        let record = record.map_err(|e| format!("read {}: {e}", path.display()))?;
        let id = record
            .get(0)
            .and_then(|b| std::str::from_utf8(b).ok())
            .and_then(|s| s.trim().parse::<u32>().ok());
        ids.push(id);
    }
    Ok(ids)
}

// ---------------------------------------------------------------------------
// Dynamic snapshot
// ---------------------------------------------------------------------------

/// Traffic state of a road segment. `Non renseigne` (unknown) segments are
/// dropped rather than represented.
#[derive(Clone, Copy)]
enum SegState {
    Fluid,
    Jam,
    Closed,
}

impl SegState {
    /// Map a Sytadin `EtatTrafic` label, ignoring the unknown state.
    fn from_label(label: &str) -> Option<Self> {
        match label.trim() {
            "Fluide" => Some(Self::Fluid),
            "Bouchon" => Some(Self::Jam),
            "Ferme" => Some(Self::Closed),
            _ => None, // "Non renseigne"
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Fluid => "fluid",
            Self::Jam => "jam",
            Self::Closed => "closed",
        }
    }
}

/// A traffic event (incident, roadwork, jam…) placed on the map.
#[derive(Serialize, ToSchema)]
pub struct TrafficEvent {
    /// Normalized category: `roadwork`, `accident`, `jam`, `weather`, `event`.
    pub category: &'static str,
    /// Human-readable label (French, as provided by the feed).
    pub label: String,
    /// Marker position `[lat, lon]` (midpoint of the first located segment).
    pub pos: [f64; 2],
    /// Expected end date/time (RFC3339-ish), when provided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

/// The dynamic part of the overlay: segment states + located events.
///
/// Carries no coordinates — clients join `states` against the geometry served
/// once by `GET /api/traffic/geometry`.
pub struct TrafficSnapshot {
    /// `ID_SEGMENT` → `"fluid"` | `"jam"` | `"closed"`.
    pub states: HashMap<u32, &'static str>,
    pub events: Vec<TrafficEvent>,
    /// Feed publication time (`DateDiffusion` on the root element, Paris local
    /// time), when the feed provides one. This is when the data was measured,
    /// which is what a user reads — not when we happened to poll for it.
    pub diffused_at: Option<String>,
}

/// Build a snapshot from the live feeds, keeping only segments the geometry can
/// actually draw. Malformed XML degrades gracefully to empty collections.
pub fn build_snapshot(
    geometry: &TrafficGeometry,
    states_xml: &[u8],
    events_xml: &[u8],
) -> TrafficSnapshot {
    let states = parse_segment_states(states_xml)
        .into_iter()
        .filter(|(id, _)| geometry.segment(*id).is_some())
        .map(|(id, state)| (id, state.as_str()))
        .collect();
    let events = parse_events(events_xml, geometry);
    TrafficSnapshot {
        states,
        events,
        diffused_at: parse_diffusion_date(states_xml),
    }
}

/// Read `DateDiffusion` off the document's root element.
///
/// Sytadin stamps it on the dynamic feeds' root (`<DonneesDynamiques… >`), but
/// does not declare it in every schema, so its absence is not an error.
fn parse_diffusion_date(xml: &[u8]) -> Option<String> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            // The root element is the first start tag of the document.
            Ok(Event::Start(e)) => {
                return e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"DateDiffusion")
                    .and_then(|a| String::from_utf8(a.value.to_vec()).ok())
                    .filter(|s| !s.trim().is_empty());
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

/// Read `ID_SEGMENT` → state from `segments_dyn.xml` (streaming).
fn parse_segment_states(xml: &[u8]) -> HashMap<u32, SegState> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut states = HashMap::new();
    let mut current_id: Option<u32> = None;
    let mut in_etat = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"SegmentDynamique" => current_id = attr_u32(&e, b"ID_SEGMENT"),
                b"EtatTrafic" => in_etat = true,
                _ => {}
            },
            Ok(Event::Text(t)) if in_etat => {
                in_etat = false;
                if let (Some(id), Ok(text)) = (current_id, t.unescape())
                    && let Some(state) = SegState::from_label(&text)
                {
                    states.insert(id, state);
                }
            }
            Ok(Event::End(e)) if e.local_name().as_ref() == b"EtatTrafic" => in_etat = false,
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!("segments_dyn.xml parse error: {e}");
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    states
}

/// Accumulator for a single `<Evenement>` while streaming `evenements.xml`.
#[derive(Default)]
struct EventBuilder {
    kind: String,
    detail: String,
    end: String,
    first_segment: Option<u32>,
}

impl EventBuilder {
    /// Finalize into a [`TrafficEvent`], if the first segment can be located.
    fn build(self, geometry: &TrafficGeometry) -> Option<TrafficEvent> {
        let pos = midpoint(geometry.segment(self.first_segment?)?);
        let label = if self.detail.trim().is_empty() {
            self.kind.clone()
        } else {
            self.detail.trim().to_string()
        };
        Some(TrafficEvent {
            category: categorize(&self.kind),
            label,
            pos,
            end: (!self.end.trim().is_empty()).then(|| self.end.trim().to_string()),
        })
    }
}

/// Parse `evenements.xml` (streaming) into located events.
fn parse_events(xml: &[u8], geometry: &TrafficGeometry) -> Vec<TrafficEvent> {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut events = Vec::new();
    let mut current: Option<EventBuilder> = None;
    let mut tag: Vec<u8> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.local_name().as_ref().to_vec();
                if name == b"Evenement" {
                    current = Some(EventBuilder::default());
                }
                tag = name;
            }
            Ok(Event::Text(t)) => {
                // Only text directly inside the tag just opened counts: the
                // indentation between two closing tags must not overwrite a
                // field already captured.
                if let Some(ev) = current.as_mut()
                    && let Ok(text) = t.unescape()
                    && !text.trim().is_empty()
                {
                    assign_event_field(ev, &tag, &text);
                }
            }
            Ok(Event::End(e)) => {
                tag.clear();
                if e.local_name().as_ref() == b"Evenement"
                    && let Some(ev) = current.take()
                    && let Some(event) = ev.build(geometry)
                {
                    events.push(event);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!("evenements.xml parse error: {e}");
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    events
}

/// Route a text node to the matching [`EventBuilder`] field by its tag.
fn assign_event_field(ev: &mut EventBuilder, tag: &[u8], text: &str) {
    match tag {
        b"QualificationTypeEvenement" => ev.kind = text.to_string(),
        b"NatureTravaux" => ev.detail = text.to_string(),
        // Keep an explicit comment only when no richer detail was captured.
        b"Commentaire" if ev.detail.trim().is_empty() => ev.detail = text.to_string(),
        b"DateFinPrevue" => ev.end = text.to_string(),
        b"Segment" if ev.first_segment.is_none() => ev.first_segment = text.trim().parse().ok(),
        _ => {}
    }
}

/// Normalize a Sytadin event qualification into a stable category slug.
fn categorize(kind: &str) -> &'static str {
    let k = kind.to_lowercase();
    if k.contains("travaux") {
        "roadwork"
    } else if k.contains("accident") {
        "accident"
    } else if k.contains("bouchon") {
        "jam"
    } else if k.contains("meteo") || k.contains("météo") {
        "weather"
    } else {
        "event"
    }
}

/// Midpoint vertex of a polyline (used to place an event marker).
fn midpoint(coords: &[[f64; 2]]) -> [f64; 2] {
    coords[coords.len() / 2]
}

/// Read an integer XML attribute by key from a start tag.
fn attr_u32(e: &BytesStart, key: &[u8]) -> Option<u32> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| {
            std::str::from_utf8(&a.value)
                .ok()
                .and_then(|s| s.trim().parse().ok())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_paris_point_falls_in_ile_de_france() {
        let from = Proj::from_proj_string(LAMBERT_2E).unwrap();
        let to = Proj::from_proj_string(WGS84).unwrap();
        // A real vertex from Segment.mif (central Île-de-France).
        let [lat, lon] = project_to_wgs84(&from, &to, 607478.71, 2419967.39).unwrap();
        assert!((48.0..49.3).contains(&lat), "lat out of range: {lat}");
        assert!((1.4..3.6).contains(&lon), "lon out of range: {lon}");
    }

    #[test]
    fn project_matches_reference_conversion() {
        let from = Proj::from_proj_string(LAMBERT_2E).unwrap();
        let to = Proj::from_proj_string(WGS84).unwrap();
        // Reference values from an independent Lambert II étendu → WGS84
        // conversion; 5e-4° ≈ 50 m, the tolerance this overlay is drawn at.
        for (x, y, ref_lat, ref_lon) in [
            (600000.0, 2200000.0, 46.799_949, 2.336_534),
            (607478.71, 2419967.39, 48.778_109, 2.438_214),
            (650000.0, 2450000.0, 49.046_106, 3.020_014),
        ] {
            let [lat, lon] = project_to_wgs84(&from, &to, x, y).unwrap();
            assert!((lat - ref_lat).abs() < 5e-4, "lat {lat} vs {ref_lat}");
            assert!((lon - ref_lon).abs() < 5e-4, "lon {lon} vs {ref_lon}");
        }
    }

    #[test]
    fn seg_state_maps_known_labels_only() {
        assert!(matches!(
            SegState::from_label("Fluide"),
            Some(SegState::Fluid)
        ));
        assert!(matches!(
            SegState::from_label("Bouchon"),
            Some(SegState::Jam)
        ));
        assert!(matches!(
            SegState::from_label("Ferme"),
            Some(SegState::Closed)
        ));
        assert!(SegState::from_label("Non renseigne").is_none());
    }

    #[test]
    fn parse_segment_states_reads_ids_and_states() {
        let xml = br#"<Root>
            <SegmentDynamique ID_SEGMENT="10"><EtatTrafic>Fluide</EtatTrafic></SegmentDynamique>
            <SegmentDynamique ID_SEGMENT="20"><EtatTrafic>Bouchon</EtatTrafic></SegmentDynamique>
            <SegmentDynamique ID_SEGMENT="30"><EtatTrafic>Non renseigne</EtatTrafic></SegmentDynamique>
        </Root>"#;
        let states = parse_segment_states(xml);
        assert_eq!(states.len(), 2);
        assert!(matches!(states.get(&10), Some(SegState::Fluid)));
        assert!(matches!(states.get(&20), Some(SegState::Jam)));
        assert!(states.get(&30).is_none());
    }

    #[test]
    fn categorize_maps_qualifications() {
        assert_eq!(categorize("Travaux"), "roadwork");
        assert_eq!(categorize("Accident matériel"), "accident");
        assert_eq!(categorize("Bouchon"), "jam");
        assert_eq!(categorize("Autre chose"), "event");
    }

    #[test]
    fn parse_mif_geometry_reads_plines() {
        let dir = tempfile::tempdir().unwrap();
        let mif = dir.path().join("Segment.mif");
        std::fs::write(
            &mif,
            "version 300\nColumns 1\n  ID_SEGMENT Integer\nData\n\n\
             pline 2\n607478.71 2419967.39\n607515.31 2419756.07\n\
             Pen (1,2,0)\n",
        )
        .unwrap();
        let geoms = parse_mif_geometry(&mif).unwrap();
        assert_eq!(geoms.len(), 1);
        assert_eq!(geoms[0].len(), 2);
        assert!((geoms[0][0][0] - 607478.71).abs() < 1e-2);
    }

    #[test]
    fn parse_mid_ids_reads_first_column() {
        let dir = tempfile::tempdir().unwrap();
        let mid = dir.path().join("Segment.mid");
        std::fs::write(
            &mid,
            "12017889,\"SEG/N6\",\"SECTION_COURANTE\",0\n\
             12017888,\"SEG, with comma\",\"X\",1\n",
        )
        .unwrap();
        let ids = parse_mid_ids(&mid).unwrap();
        assert_eq!(ids, vec![Some(12017889), Some(12017888)]);
    }

    #[test]
    fn build_snapshot_joins_states_with_geometry() {
        let mut segments = HashMap::new();
        segments.insert(20u32, vec![[48.85, 2.35], [48.86, 2.36]]);
        let geometry = TrafficGeometry { segments };

        let states = br#"<R><SegmentDynamique ID_SEGMENT="20">
            <EtatTrafic>Bouchon</EtatTrafic></SegmentDynamique>
            <SegmentDynamique ID_SEGMENT="99">
            <EtatTrafic>Fluide</EtatTrafic></SegmentDynamique></R>"#;
        let events = br#"<R><Evenement ID_EVT="1">
            <TypeEvenement><QualificationTypeEvenement>Travaux</QualificationTypeEvenement>
            <Travaux><NatureTravaux>Fermeture</NatureTravaux></Travaux></TypeEvenement>
            <DateFinPrevue>2026-09-04T07:45:17</DateFinPrevue>
            <Localisation><Segments><Segment>20</Segment></Segments></Localisation>
            </Evenement></R>"#;

        let snap = build_snapshot(&geometry, states, events);
        // Only segment 20 has geometry; 99 is dropped.
        assert_eq!(snap.states.len(), 1);
        assert_eq!(snap.states.get(&20), Some(&"jam"));
        assert_eq!(snap.events.len(), 1);
        assert_eq!(snap.events[0].category, "roadwork");
        assert_eq!(snap.events[0].label, "Fermeture");
        assert_eq!(snap.events[0].end.as_deref(), Some("2026-09-04T07:45:17"));
    }

    #[test]
    fn parse_events_keeps_fields_across_indentation() {
        let mut segments = HashMap::new();
        segments.insert(20u32, vec![[48.85, 2.35], [48.86, 2.36]]);
        let geometry = TrafficGeometry { segments };

        // Indentation between closing and opening tags must not overwrite a
        // captured field, and a Commentaire must not shadow a richer detail.
        let xml = br#"<R>
            <Evenement>
                <TypeEvenement>
                    <QualificationTypeEvenement>Accident</QualificationTypeEvenement>
                </TypeEvenement>
                <Commentaire>Voie de droite neutralisee</Commentaire>
                <Localisation><Segments><Segment>20</Segment></Segments></Localisation>
            </Evenement>
        </R>"#;

        let events = parse_events(xml, &geometry);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].category, "accident");
        assert_eq!(events[0].label, "Voie de droite neutralisee");
        assert!(events[0].end.is_none());
    }

    #[test]
    fn parse_diffusion_date_reads_root_attribute() {
        let xml = br#"<DonneesDynamiquesSegments VersionConfiguration="261"
            DateDiffusion="2026-08-13T01:08:17"><SegmentDynamique ID_SEGMENT="1"/>
            </DonneesDynamiquesSegments>"#;
        assert_eq!(
            parse_diffusion_date(xml).as_deref(),
            Some("2026-08-13T01:08:17")
        );
    }

    #[test]
    fn parse_diffusion_date_absent_or_malformed_yields_none() {
        assert!(parse_diffusion_date(br#"<R><S ID="1"/></R>"#).is_none());
        assert!(parse_diffusion_date(br#"<R DateDiffusion="  "/>"#).is_none());
        assert!(parse_diffusion_date(b"not xml at all").is_none());
    }

    #[test]
    fn build_snapshot_carries_the_feed_publication_time() {
        let geometry = TrafficGeometry {
            segments: HashMap::new(),
        };
        let states = br#"<Root DateDiffusion="2026-08-13T01:08:17"></Root>"#;
        let snap = build_snapshot(&geometry, states, b"<R></R>");
        assert_eq!(snap.diffused_at.as_deref(), Some("2026-08-13T01:08:17"));
    }

    #[test]
    fn parse_events_skips_events_without_known_segment() {
        let geometry = TrafficGeometry {
            segments: HashMap::new(),
        };
        let xml = br#"<R><Evenement>
            <QualificationTypeEvenement>Travaux</QualificationTypeEvenement>
            <Segment>404</Segment></Evenement></R>"#;
        assert!(parse_events(xml, &geometry).is_empty());
    }
}
