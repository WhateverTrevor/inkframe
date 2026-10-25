//! The first-run tour: a few cards that point out the parts of the window,
//! one at a time, the first time Inkframe opens (and again from
//! Help › Take the Tour).
//!
//! Each card is about one part of the window. While it shows, the rest of
//! the window is dimmed, with that part left lit and outlined, and the
//! card sits beside it. The app notes where each part was drawn every
//! frame (`Targets`), so the tour finds them wherever the person has
//! moved them; a part that isn't on screen (a closed timeline, say) just
//! gets its card in the middle of the window, unlit.
//!
//! The dimming covers the whole window and takes the clicks, so nothing
//! changes behind the tour; the app also leaves its shortcuts alone while
//! the tour is open, and the tour takes Enter and the arrow keys.

use egui::{Align2, Color32, Context, Id, Key, Order, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::theme;

/// Where the parts of the window the tour points at were drawn this
/// frame, if they were.
#[derive(Debug, Default, Clone, Copy)]
pub struct Targets {
    pub tools: Option<Rect>,
    pub context_bar: Option<Rect>,
    pub stage: Option<Rect>,
    pub properties: Option<Rect>,
    pub timeline: Option<Rect>,
    pub finder: Option<Rect>,
    pub help: Option<Rect>,
}

impl Targets {
    /// Notes a part's place, keeping the first if there are several (two
    /// stages, say): the tour only needs one to point at.
    pub fn note(slot: &mut Option<Rect>, rect: Rect) {
        slot.get_or_insert(rect);
    }
}

/// What a card points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// Nothing: a card in the middle of the window.
    Window,
    Tools,
    ContextBar,
    Stage,
    Properties,
    Timeline,
    Finder,
    Help,
}

struct Step {
    target: Target,
    title: &'static str,
    text: &'static str,
}

const STEPS: [Step; 8] = [
    Step {
        target: Target::Window,
        title: "Welcome to Inkframe",
        text: "A quick look around the window: seven stops, under a minute. \
               Press Enter or → to go on, and Esc to leave the tour at any time.",
    },
    Step {
        target: Target::Tools,
        title: "The tools",
        text: "Drawing, painting, rigging and the camera. Hover over a tool to see what it does. \
               A small triangle means other tools share its slot: right-click it, or press the \
               tool's key with Shift.",
    },
    Step {
        target: Target::ContextBar,
        title: "What you can do now",
        text: "This bar follows what you're doing: the tool you have, and buttons for what can be \
               done with what's selected, such as making a symbol or bending a drawing. Its right \
               end switches rulers, onion skins and the light table.",
    },
    Step {
        target: Target::Stage,
        title: "The stage",
        text: "Your picture. What's inside the frame is what gets exported. Scroll to pan, pinch \
               or ⌘-scroll to zoom, and ⌘0 fits it in view again.",
    },
    Step {
        target: Target::Properties,
        title: "Properties",
        text: "Settings for what you're working on: the current Tool, the selected Layer, and the \
               whole Document. Every area's name is a menu: click it to show something else there, \
               such as the Library.",
    },
    Step {
        target: Target::Timeline,
        title: "The timeline",
        text: "Your layers, and their keyframes over time. + adds a layer to draw into. Move to \
               another frame and change a layer, and it's keyed there. Enter plays.",
    },
    Step {
        target: Target::Finder,
        title: "Find anything",
        text: "Can't find a command? Press ⌘K and type its name, or what it's for: \"bulge\", \
               \"rig\", \"web page\". It also shows which menu it's in and its keys.",
    },
    Step {
        target: Target::Help,
        title: "Learning more",
        text: "Help has Inkframe for Artists, a beginner's guidebook with a chapter for each group \
               of tools, and this tour again. File › Open Example has finished projects to take \
               apart. Have fun!",
    },
];

/// How the tour ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// Left partway, or finished with Done.
    Closed,
    /// Finished by asking to see the examples.
    OpenExamples,
}

#[derive(Debug, Default)]
pub struct Tour {
    /// The card showing, or `None` with the tour closed.
    step: Option<usize>,
    /// The card's size last frame, to place it before it's drawn.
    card: Vec2,
}

const CARD_WIDTH: f32 = 320.0;
/// The gap between the lit part and its outline, and between the outline
/// and the card.
const MARGIN: f32 = 6.0;
const GAP: f32 = 14.0;

impl Tour {
    pub fn start(&mut self) {
        self.step = Some(0);
    }

    /// Starts at a card, counting from 1, for screenshots.
    pub fn start_at(&mut self, card: usize) {
        self.step = Some(card.clamp(1, STEPS.len()) - 1);
    }

    pub fn is_open(&self) -> bool {
        self.step.is_some()
    }

    /// Takes the keys the tour answers to, before the app's shortcuts see
    /// them. Returns how the tour ended, if a key ended it.
    pub fn keys(&mut self, ctx: &Context) -> Option<Ended> {
        let step = self.step?;
        let pressed = |key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
        if pressed(Key::Escape) {
            self.step = None;
            return Some(Ended::Closed);
        }
        if pressed(Key::Enter) || pressed(Key::ArrowRight) {
            return self.next(step);
        }
        if pressed(Key::ArrowLeft) {
            self.step = Some(step.saturating_sub(1));
        }
        None
    }

    fn next(&mut self, step: usize) -> Option<Ended> {
        if step + 1 < STEPS.len() {
            self.step = Some(step + 1);
            None
        } else {
            self.step = None;
            Some(Ended::Closed)
        }
    }

    /// Draws the open card over the window. Returns how the tour ended, if
    /// it ended this frame.
    pub fn show(&mut self, ctx: &Context, targets: &Targets) -> Option<Ended> {
        let step = self.step?;
        let window = ctx.content_rect();
        let lit = match STEPS[step].target {
            Target::Window => None,
            Target::Tools => targets.tools,
            Target::ContextBar => targets.context_bar,
            Target::Stage => targets.stage,
            Target::Properties => targets.properties,
            Target::Timeline => targets.timeline,
            Target::Finder => targets.finder,
            Target::Help => targets.help,
        }
        .map(|rect| rect.expand(MARGIN).intersect(window));

        // The shade: everything but the lit part, dimmed, taking the clicks.
        egui::Area::new(Id::new("tour_shade")).order(Order::Foreground).fixed_pos(window.min).show(
            ctx,
            |ui| {
                ui.allocate_rect(window, Sense::click_and_drag());
                let painter = ui.painter();
                let dim = Color32::from_black_alpha(150);
                match lit {
                    Some(lit) => {
                        for rect in around(window, lit) {
                            painter.rect_filled(rect, 0.0, dim);
                        }
                        painter.rect_stroke(lit, 6.0, Stroke::new(2.0, theme::ACCENT), StrokeKind::Outside);
                    }
                    None => {
                        painter.rect_filled(window, 0.0, dim);
                    }
                }
            },
        );

        let size = if self.card == Vec2::ZERO { Vec2::new(CARD_WIDTH, 180.0) } else { self.card };
        let at = place(window, lit, size);
        let mut ended = None;
        let card =
            egui::Area::new(Id::new("tour_card")).order(Order::Foreground).fixed_pos(at).show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .stroke(Stroke::new(1.0, theme::ACCENT))
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.set_width(CARD_WIDTH - 32.0);
                        ended = self.card_contents(ui, step);
                    });
            });
        // On top of the shade, which was shown first.
        ctx.move_to_top(card.response.layer_id);
        self.card = card.response.rect.size();
        ended
    }

    fn card_contents(&mut self, ui: &mut egui::Ui, step: usize) -> Option<Ended> {
        let Step { title, text, .. } = &STEPS[step];
        ui.weak(format!("{} of {}", step + 1, STEPS.len()));
        ui.label(egui::RichText::new(*title).size(17.0).strong().color(theme::TEXT));
        ui.add_space(4.0);
        ui.label(*text);
        ui.add_space(10.0);
        let last = step + 1 == STEPS.len();
        let mut ended = None;
        ui.horizontal(|ui| {
            if !last && ui.button("Skip tour").clicked() {
                self.step = None;
                ended = Some(Ended::Closed);
            }
            if last && ui.button("Open an example").clicked() {
                self.step = None;
                ended = Some(Ended::OpenExamples);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let next = if step == 0 {
                    "Start"
                } else if last {
                    "Done"
                } else {
                    "Next"
                };
                if ui.add(egui::Button::new(next).fill(theme::ACCENT_DIM)).clicked() {
                    ended = self.next(step);
                }
                if step > 0 && ui.button("Back").clicked() {
                    self.step = Some(step - 1);
                }
            });
        });
        ended
    }
}

/// The parts of `window` outside `lit`: above, below, left and right.
fn around(window: Rect, lit: Rect) -> [Rect; 4] {
    [
        Rect::from_min_max(window.min, Pos2::new(window.max.x, lit.min.y)),
        Rect::from_min_max(Pos2::new(window.min.x, lit.max.y), window.max),
        Rect::from_min_max(Pos2::new(window.min.x, lit.min.y), Pos2::new(lit.min.x, lit.max.y)),
        Rect::from_min_max(Pos2::new(lit.max.x, lit.min.y), Pos2::new(window.max.x, lit.max.y)),
    ]
}

/// Where a card of `size` goes: beside the lit part (right of it, left,
/// below or above, the first that fits in the window), or in the middle of
/// it if it's too big to sit beside (the stage), or of the window.
fn place(window: Rect, lit: Option<Rect>, size: Vec2) -> Pos2 {
    let inside = window.shrink(8.0);
    let centred_in = |rect: Rect| Align2::CENTER_CENTER.align_size_within_rect(size, rect).min;
    let Some(lit) = lit else { return centred_in(window) };
    // Beside it, lined up with its middle, then pulled back into the window.
    let candidates = [
        Pos2::new(lit.max.x + GAP, lit.center().y - size.y / 2.0),
        Pos2::new(lit.min.x - GAP - size.x, lit.center().y - size.y / 2.0),
        Pos2::new(lit.center().x - size.x / 2.0, lit.max.y + GAP),
        Pos2::new(lit.center().x - size.x / 2.0, lit.min.y - GAP - size.y),
    ];
    for at in candidates {
        let card = Rect::from_min_size(at, size);
        let slid = card.translate(shift_into(card, inside));
        if inside.contains_rect(slid) && !slid.intersects(lit) {
            return slid.min;
        }
    }
    centred_in(lit.intersect(window))
}

/// How far to move `card` so it's within `bounds` (when it fits at all).
fn shift_into(card: Rect, bounds: Rect) -> Vec2 {
    let axis = |min: f32, max: f32, low: f32, high: f32| {
        if min < low {
            low - min
        } else if max > high {
            high - max
        } else {
            0.0
        }
    };
    Vec2::new(
        axis(card.min.x, card.max.x, bounds.min.x, bounds.max.x),
        axis(card.min.y, card.max.y, bounds.min.y, bounds.max.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Rect = Rect { min: Pos2::new(0.0, 0.0), max: Pos2::new(1600.0, 1000.0) };
    const CARD: Vec2 = Vec2::new(320.0, 180.0);

    #[test]
    fn cards_sit_beside_what_they_point_at() {
        // The toolbar, down the left: the card goes to its right.
        let tools = Rect::from_min_max(Pos2::new(0.0, 40.0), Pos2::new(70.0, 700.0));
        let at = place(WINDOW, Some(tools), CARD);
        assert!(at.x >= tools.max.x + GAP);
        // Properties, down the right: to its left.
        let properties = Rect::from_min_max(Pos2::new(1300.0, 40.0), Pos2::new(1600.0, 700.0));
        assert!(place(WINDOW, Some(properties), CARD).x + CARD.x <= properties.min.x);
        // The timeline, along the bottom and the whole width: above it.
        let timeline = Rect::from_min_max(Pos2::new(0.0, 720.0), Pos2::new(1600.0, 1000.0));
        assert!(place(WINDOW, Some(timeline), CARD).y + CARD.y <= timeline.min.y);
        // A button in the menu bar's corner: beside it, kept in the window.
        let finder = Rect::from_min_max(Pos2::new(1400.0, 0.0), Pos2::new(1550.0, 24.0));
        let card = Rect::from_min_size(place(WINDOW, Some(finder), CARD), CARD);
        assert!(WINDOW.contains_rect(card) && !card.intersects(finder));
    }

    #[test]
    fn a_card_with_no_room_beside_goes_in_the_middle() {
        // A stage filling all but the toolbar, with no room around it.
        let stage = Rect::from_min_max(Pos2::new(70.0, 40.0), Pos2::new(1300.0, 950.0));
        let card = Rect::from_min_size(place(WINDOW, Some(stage), CARD), CARD);
        assert!((card.center() - stage.center()).length() < 1.0);
        let card = Rect::from_min_size(place(WINDOW, None, CARD), CARD);
        assert!((card.center() - WINDOW.center()).length() < 1.0);
    }

    #[test]
    fn the_shade_leaves_just_the_lit_part() {
        let lit = Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(300.0, 200.0));
        let shade = around(WINDOW, lit);
        let area: f32 = shade.iter().map(Rect::area).sum();
        assert!((area + lit.area() - WINDOW.area()).abs() < 1.0);
        assert!(shade.iter().all(|rect| !rect.intersects(lit.shrink(0.5))));
    }

    /// What the test window holds: the tour, how it ended, and how often
    /// the button behind it was clicked.
    #[derive(Default)]
    struct Window {
        tour: Tour,
        ended: Option<Ended>,
        clicks: usize,
    }

    #[test]
    fn the_tour_is_clicked_and_keyed_through_while_the_window_waits() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let mut window = Window::default();
        window.tour.start();
        let mut h = Harness::new_ui_state(
            |ui, window: &mut Window| {
                let ctx = &ui.ctx().clone();
                let mut targets = Targets::default();
                let behind = ui.button("Behind");
                if behind.clicked() {
                    window.clicks += 1;
                }
                Targets::note(&mut targets.tools, behind.rect);
                if let Some(ended) = window.tour.keys(ctx) {
                    window.ended = Some(ended);
                }
                if let Some(ended) = window.tour.show(ctx, &targets) {
                    window.ended = Some(ended);
                }
            },
            window,
        );
        h.set_size(Vec2::new(1000.0, 700.0));
        h.run();
        // The shade takes the click meant for the button behind it.
        h.get_by_label("Behind").click();
        h.run();
        assert_eq!(h.state().clicks, 0);

        h.get_by_label("Start").click();
        h.run();
        assert_eq!(h.state().tour.step, Some(1));
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(h.state().tour.step, Some(2));
        h.get_by_label("Back").click();
        h.run();
        assert_eq!(h.state().tour.step, Some(1));
        h.key_press(Key::Escape);
        h.run();
        assert_eq!(h.state().ended, Some(Ended::Closed));
        assert!(!h.state().tour.is_open());

        // With the tour gone, the button works again.
        h.get_by_label("Behind").click();
        h.run();
        assert_eq!(h.state().clicks, 1);
    }

    #[test]
    fn the_keys_step_through_and_out() {
        let mut tour = Tour::default();
        tour.start();
        assert_eq!(tour.next(0), None);
        assert_eq!(tour.step, Some(1));
        tour.start_at(STEPS.len());
        assert_eq!(tour.next(STEPS.len() - 1), Some(Ended::Closed));
        assert!(!tour.is_open());
    }
}
