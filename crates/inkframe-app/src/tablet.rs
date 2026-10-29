//! Pen tablets: how hard the pen is pressed, and how it's tilted.
//!
//! egui reports where the pointer is but not pen pressure, so it's fetched
//! from each platform as it comes:
//!
//! * **Windows:** Windows Ink reports pens as pointer events with pressure,
//!   which winit passes on as touches with a force, and egui as
//!   `Event::Touch` with `force`. Nothing extra is needed; see
//!   [`samples`].
//! * **macOS:** AppKit puts pressure and tilt on the mouse events a pen
//!   sends (and on separate tablet events), but winit drops them. A local
//!   event monitor, installed once by [`install`], sees each event before
//!   the app does, notes the pen's state, and passes the event on
//!   untouched.
//!
//! A mouse reports no pressure and paints at full strength.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use egui::Pos2;

/// The pen's state, as last reported.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pen {
    /// 0 (barely touching) to 1 (pressed hard).
    pub pressure: f32,
    /// Sideways and forward lean, each −1 to 1 (0 is upright).
    pub tilt: (f32, f32),
}

/// What the macOS monitor saw last: `None` once a mouse moves instead.
static LATEST: Mutex<Option<Pen>> = Mutex::new(None);

/// The Pressure setting (see [`set_feel`]), as an `f32`'s bits.
static FEEL: AtomicU32 = AtomicU32::new(0);

/// How pressing the pen feels, from −1 (soft: a light touch paints fully)
/// through 0 (as the pen reports it) to 1 (firm: it takes pressing hard),
/// as the pressure curves of Krita and Clip Studio set. Every tool reads
/// pressure through here, so it applies to them all.
pub fn set_feel(feel: f32) {
    FEEL.store(feel.clamp(-1.0, 1.0).to_bits(), Ordering::Relaxed);
}

/// A pen's pressure as the Pressure setting shapes it.
fn felt(pressure: f32) -> f32 {
    shaped(pressure, f32::from_bits(FEEL.load(Ordering::Relaxed)))
}

/// `pressure` raised to a power from ½ (soft) to 2 (firm) as `feel` goes
/// from −1 to 1, which bends the curve but keeps 0 and 1 where they are.
fn shaped(pressure: f32, feel: f32) -> f32 {
    pressure.clamp(0.0, 1.0).powf(2f32.powf(feel))
}

/// The pen's state, if a pen (rather than a mouse) is in use.
pub fn current() -> Option<Pen> {
    let pen = (*LATEST.lock().expect("not poisoned"))?;
    Some(Pen { pressure: felt(pen.pressure), ..pen })
}

/// Starts listening for pen events. Call once, at startup.
pub fn install() {
    #[cfg(target_os = "macos")]
    macos::install();
}

/// One point along the pointer's path this frame, with how hard the pen
/// was pressed there (1 for a mouse).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub pos: Pos2,
    pub pressure: f32,
    /// The pen's lean (see [`Pen::tilt`]); upright for a mouse.
    pub tilt: (f32, f32),
    /// Whether a pen made it, rather than a mouse.
    pub stylus: bool,
}

impl Sample {
    /// A sample at `pos` with the pen's state now (full pressure, upright,
    /// with a mouse).
    pub fn now(pos: Pos2) -> Self {
        let pen = current();
        Self {
            pos,
            pressure: pen.map_or(1.0, |p| p.pressure),
            tilt: pen.map_or((0.0, 0.0), |p| p.tilt),
            stylus: pen.is_some(),
        }
    }
}

/// Every point the pointer passed through this frame, in order, with its
/// pressure. Several events often arrive between two frames; painting
/// through all of them, rather than just where the pointer ended up, keeps
/// fast strokes smooth.
pub fn samples(input: &egui::InputState) -> Vec<Sample> {
    // A pen on Windows arrives as touches with a force. When there are
    // any, they're the pen's path; the pointer events egui makes from
    // them would only repeat it without the pressure.
    let touches: Vec<Sample> = input
        .events
        .iter()
        .filter_map(|event| match event {
            egui::Event::Touch { pos, force: Some(force), .. } => {
                Some(Sample { pos: *pos, pressure: felt(*force), tilt: (0.0, 0.0), stylus: true })
            }
            _ => None,
        })
        .collect();
    if !touches.is_empty() {
        return touches;
    }
    let pressure = current().map_or(1.0, |pen| pen.pressure);
    let tilt = current().map_or((0.0, 0.0), |pen| pen.tilt);
    let stylus = current().is_some();
    input
        .events
        .iter()
        .filter_map(|event| match event {
            egui::Event::PointerMoved(pos) => Some(Sample { pos: *pos, pressure, tilt, stylus }),
            _ => None,
        })
        .collect()
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventSubtype, NSEventType};

    use super::{LATEST, Pen};

    pub fn install() {
        let mask = NSEventMask::LeftMouseDown
            | NSEventMask::LeftMouseDragged
            | NSEventMask::LeftMouseUp
            | NSEventMask::MouseMoved
            | NSEventMask::TabletPoint;
        let handler = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
            // SAFETY: AppKit hands the monitor a valid event for the length
            // of the call.
            let event = unsafe { event.as_ref() };
            record(event);
            // Returning the event unchanged lets it carry on to the app.
            (event as *const NSEvent).cast_mut()
        });
        // SAFETY: the handler returns the event it was given, which is a
        // valid pointer.
        let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &handler) };
        // The monitor is wanted for as long as the app runs, so it's never
        // removed.
        std::mem::forget(monitor);
    }

    fn record(event: &NSEvent) {
        let kind = event.r#type();
        // A pen's mouse events say so with the "tablet point" subtype;
        // tablet point events are always the pen's.
        let from_pen = kind == NSEventType::TabletPoint || event.subtype() == NSEventSubtype::TabletPoint;
        let pen = from_pen.then(|| {
            let tilt = event.tilt();
            // A pen hovering, or the instant it lifts, reports no pressure;
            // that isn't a stroke, so it's left at what it was.
            Pen { pressure: event.pressure(), tilt: (tilt.x as f32, tilt.y as f32) }
        });
        *LATEST.lock().expect("not poisoned") = pen;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(events: Vec<egui::Event>) -> egui::InputState {
        let raw = egui::RawInput { events, ..Default::default() };
        egui::InputState::default().begin_pass(raw, false, 1.0, egui::InputOptions::default())
    }

    #[test]
    fn the_pressure_setting_bends_the_curve_but_not_its_ends() {
        for feel in [-1.0, 0.0, 1.0] {
            assert_eq!((shaped(0.0, feel), shaped(1.0, feel)), (0.0, 1.0));
        }
        assert!((shaped(0.25, -1.0) - 0.5).abs() < 1e-5, "soft: a quarter press paints half");
        assert!((shaped(0.5, 1.0) - 0.25).abs() < 1e-5, "firm: half a press paints a quarter");
        assert_eq!(shaped(0.4, 0.0), 0.4);
    }

    #[test]
    fn a_mouse_paints_at_full_pressure_through_every_move() {
        let moves = [egui::pos2(1.0, 1.0), egui::pos2(5.0, 1.0)].map(egui::Event::PointerMoved).to_vec();
        let samples = samples(&input(moves));
        assert_eq!(samples.len(), 2);
        assert!(samples.iter().all(|s| s.pressure == 1.0));
    }

    #[test]
    fn a_windows_pen_takes_its_pressure_from_touches() {
        let touch = |x: f32, force: f32| egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(1),
            phase: egui::TouchPhase::Move,
            pos: egui::pos2(x, 0.0),
            force: Some(force),
        };
        let events = vec![touch(1.0, 0.2), egui::Event::PointerMoved(egui::pos2(1.0, 0.0)), touch(2.0, 0.6)];
        let samples = samples(&input(events));
        assert_eq!(samples.iter().map(|s| s.pressure).collect::<Vec<_>>(), [0.2, 0.6]);
    }
}
