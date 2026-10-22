//! A web page that plays the animation, as Animate's HTML5 Canvas export
//! does: one self-contained `.html` file to open in any browser or put on
//! a site, with nothing else to load.
//!
//! Every frame is drawn as SVG (see `svg.rs`), so the page stays sharp at
//! any size, and frames that look the same are kept once. A few lines of
//! script swap them at the document's frame rate, looping; clicking the
//! animation pauses and plays it, and the arrow keys step a frame when
//! paused. It fits the window, keeping its shape.
//!
//! It can be interactive, as Animate's is: frames can stop the animation
//! or send it to another frame, and layers can be buttons that do those or
//! open a link (see `web_actions.rs` in the core). Buttons light up under
//! the pointer. A page with any of those leaves pausing to them.

use std::fmt::Write;
use std::ops::ControlFlow;

use inkframe_core::Document;
use inkframe_render::ImageLibrary;

use inkframe_core::WebAction;

use crate::svg::frame_svg;
use crate::{ExportError, ExportSettings, Progress};

pub(crate) fn write(
    doc: &Document,
    images: &ImageLibrary,
    settings: &ExportSettings,
    mut on_progress: impl FnMut(Progress) -> ControlFlow<()>,
) -> Result<(), ExportError> {
    let frames_total = settings.frames.end() - settings.frames.start() + 1;
    let mut frames = Vec::new();
    for (done, frame) in settings.frames.clone().enumerate() {
        frames.push(frame_svg(doc, images, frame, settings.scale, settings.transparent));
        if on_progress(Progress { frames_done: done as u32 + 1, frames_total }).is_break() {
            return Err(ExportError::Cancelled);
        }
    }
    let title = settings.output.file_stem().map_or("Animation".into(), |s| s.to_string_lossy().into_owned());
    let start = *settings.frames.start();
    // Frame actions by their place among the exported frames; buttons by
    // the id the SVG marks them with.
    let interaction = Interaction {
        frames: doc
            .frame_actions
            .iter()
            .filter(|a| settings.frames.contains(&a.frame))
            .map(|a| ((a.frame - start) as usize, shift(&a.action, start)))
            .collect(),
        buttons: doc
            .all_layers()
            .filter_map(|l| Some((crate::svg::button_id(l.id), shift(l.button.as_ref()?, start))))
            .collect(),
    };
    let page =
        page(&title, &frames, doc.frame_rate.fps(), doc.background, settings.transparent, &interaction);
    std::fs::write(&settings.output, page)?;
    Ok(())
}

/// What the page does as it plays and when it's clicked: actions on frames
/// (by their place in the page) and on buttons (by id).
#[derive(Debug, Default)]
pub struct Interaction {
    pub frames: Vec<(usize, WebAction)>,
    pub buttons: Vec<(i64, WebAction)>,
}

/// `action` with the frames it goes to counted from the first exported
/// frame, as the page counts them.
fn shift(action: &WebAction, start: u32) -> WebAction {
    match action {
        WebAction::GotoAndPlay(f) => WebAction::GotoAndPlay(f.saturating_sub(start)),
        WebAction::GotoAndStop(f) => WebAction::GotoAndStop(f.saturating_sub(start)),
        other => other.clone(),
    }
}

/// The page: the frames (each different one once, and which to show
/// when), and the script that plays them.
pub fn page(
    title: &str,
    frames: &[String],
    fps: f64,
    background: inkframe_core::Color,
    transparent: bool,
    interaction: &Interaction,
) -> String {
    let mut unique: Vec<&str> = Vec::new();
    let order: Vec<usize> = frames
        .iter()
        .map(|svg| match unique.iter().position(|u| *u == svg) {
            Some(i) => i,
            None => {
                unique.push(svg);
                unique.len() - 1
            }
        })
        .collect();
    // As JSON inside the page, with `</` broken up so no frame can end the
    // script it's in early.
    let actions: serde_json::Map<String, serde_json::Value> =
        interaction.frames.iter().map(|(i, a)| (i.to_string(), serde_json::json!(a))).collect();
    let buttons: serde_json::Map<String, serde_json::Value> =
        interaction.buttons.iter().map(|(id, a)| (id.to_string(), serde_json::json!(a))).collect();
    let data = serde_json::json!({ "fps": fps, "frames": unique, "order": order, "actions": actions, "buttons": buttons })
        .to_string()
        .replace("</", "<\\/");
    // The page is the stage's colour, round the animation too; a
    // transparent export shows the page through, as it would on a site.
    let page_colour = format!("rgb({}, {}, {})", background.r, background.g, background.b);
    let backdrop = if transparent { "transparent".to_owned() } else { page_colour.clone() };
    let mut html = String::new();
    let _ = write!(
        html,
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  html, body {{ margin: 0; height: 100%; background: {page_colour}; }}
  body {{ display: flex; align-items: center; justify-content: center; }}
  #stage {{ width: 100vw; max-height: 100vh; background: {backdrop}; cursor: pointer; }}
  #stage svg {{ display: block; width: 100%; height: auto; max-height: 100vh; }}
  .inkframe-button {{ cursor: pointer; }}
  .inkframe-button:hover {{ filter: brightness(1.15); }}
  .inkframe-button:active {{ filter: brightness(0.85); }}
</style>
</head>
<body>
<div id="stage" title="Click to pause or play; arrow keys step while paused"></div>
<script type="application/json" id="animation">{data}</script>
<script>
(() => {{
  const animation = JSON.parse(document.getElementById("animation").textContent);
  const stage = document.getElementById("stage");
  const count = animation.order.length;
  // With actions or buttons, they decide when it plays; a click elsewhere
  // does nothing.
  const interactive = Object.keys(animation.actions).length + Object.keys(animation.buttons).length > 0;
  let frame = 0, playing = true, shown = -1, last = performance.now(), owed = 0;
  const show = () => {{
    const which = animation.order[frame];
    if (which !== shown) {{ stage.innerHTML = animation.frames[which]; shown = which; }}
  }};
  const go = (to) => {{ frame = Math.min(Math.max(to, 0), count - 1); owed = 0; show(); }};
  // An action: "Play", "Stop", {{"GotoAndPlay": n}}, {{"GotoAndStop": n}}
  // or {{"OpenLink": address}}.
  const run = (action) => {{
    if (action === "Play") playing = true;
    else if (action === "Stop") playing = false;
    else if (action.GotoAndPlay !== undefined) {{ go(action.GotoAndPlay); playing = true; }}
    else if (action.GotoAndStop !== undefined) {{ go(action.GotoAndStop); playing = false; }}
    else if (action.OpenLink && /^(https?:|mailto:)/i.test(action.OpenLink)) window.open(action.OpenLink, "_blank", "noopener");
  }};
  const tick = (now) => {{
    // Frames advance by time, not by screen refreshes, so the timing holds
    // on any display; each frame passed runs its action, if it has one.
    if (playing) {{
      owed += (now - last) * animation.fps / 1000;
      while (owed >= 1 && playing) {{
        owed -= 1;
        frame = (frame + 1) % count;
        show();
        const action = animation.actions[frame];
        if (action) {{ run(action); if (typeof action === "object" && action.GotoAndPlay !== undefined) break; }}
      }}
    }}
    last = now;
    requestAnimationFrame(tick);
  }};
  stage.addEventListener("click", (event) => {{
    const button = event.target.closest("[data-button]");
    if (button) {{ const action = animation.buttons[button.dataset.button]; if (action) run(action); }}
    else if (!interactive) {{ playing = !playing; owed = 0; }}
  }});
  document.addEventListener("keydown", (event) => {{
    if (playing) return;
    if (event.key === "ArrowRight") frame = (frame + 1) % count;
    else if (event.key === "ArrowLeft") frame = (frame + count - 1) % count;
    else return;
    show();
  }});
  show();
  if (animation.actions[0]) run(animation.actions[0]);
  requestAnimationFrame(tick);
}})();
</script>
</body>
</html>
"#
    );
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_that_look_the_same_are_kept_once() {
        let frames = ["<svg>a</svg>", "<svg>a</svg>", "<svg>b</svg>", "<svg>a</svg>"].map(String::from);
        let html = page("Walk", &frames, 24.0, inkframe_core::Color::WHITE, false, &Interaction::default());
        assert!(html.contains(r#""order":[0,0,1,0]"#), "{html}");
        assert_eq!(html.matches("<svg>a").count(), 1);
        assert!(!html.contains("</svg>\""), "closing tags are broken up inside the script");
        assert!(html.contains("<title>Walk</title>"));
        assert!(html.contains("rgb(255, 255, 255)"));
    }

    #[test]
    fn buttons_and_frame_actions_go_into_the_page() {
        use inkframe_core::{Color, Document, Element, Point, Shape, Style, Transform, WebAction};
        let mut doc = Document::default();
        let shape = Element {
            shape: Shape::Rect { width: 100.0, height: 40.0, corner_radius: 8.0 },
            style: Style { fill: Some(Color::rgb(80, 160, 90)), stroke: None, gradient: None },
        };
        let id = doc.add_layer("Replay", vec![shape], 0, Transform::at(Point::new(300.0, 300.0)));
        doc.layer_mut(id).unwrap().button = Some(WebAction::GotoAndPlay(4));
        doc.set_frame_action(9, Some(WebAction::Stop));
        let dir = std::env::temp_dir().join("inkframe-html-buttons");
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("buttons.html");
        // Exporting frames 2 to 12, so every frame counts from 2.
        let settings = ExportSettings {
            format: crate::ExportFormat::Html,
            output: output.clone(),
            frames: 2..=12,
            scale: 1.0,
            transparent: false,
            project_dir: None,
        };
        write(&doc, &ImageLibrary::default(), &settings, |_| ControlFlow::Continue(())).unwrap();
        let html = std::fs::read_to_string(&output).unwrap();
        let button = crate::svg::button_id(id);
        assert!(
            html.contains(&format!(r#"class=\"inkframe-button\" data-button=\"{button}\""#)),
            "the button's marked"
        );
        assert!(html.contains(&format!(r#""buttons":{{"{button}":{{"GotoAndPlay":2}}}}"#)), "{html}");
        assert!(html.contains(r#""actions":{"7":"Stop"}"#), "frame 10 is the page's 8th");
    }

    /// Writes an interactive sample to `target/web-check/buttons.html`, to
    /// try in a browser: a ball rolls across and stops at the end (a frame
    /// action); Replay sends it back to the start, Pause stops it.
    #[test]
    #[ignore = "writes a page to look at by hand"]
    fn write_interactive_sample() {
        use inkframe_core::{Color, Document, Element, Point, Shape, Style, Transform, WebAction};
        let mut doc = Document::default();
        doc.background = Color::rgb(245, 240, 230);
        let fill = |c: Color| Style { fill: Some(c), stroke: None, gradient: None };
        let ball = Element {
            shape: Shape::Ellipse { radius_x: 60.0, radius_y: 60.0 },
            style: fill(Color::rgb(230, 90, 100)),
        };
        let id = doc.add_layer("Ball", vec![ball], 0, Transform::at(Point::new(200.0, 500.0)));
        doc.layer_mut(id).unwrap().track.set_pose(47, Transform::at(Point::new(1700.0, 500.0)));
        let button = |c: Color| Element {
            shape: Shape::Rect { width: 220.0, height: 80.0, corner_radius: 20.0 },
            style: fill(c),
        };
        let replay = doc.add_layer(
            "Replay",
            vec![button(Color::rgb(80, 160, 90))],
            0,
            Transform::at(Point::new(760.0, 850.0)),
        );
        let pause = doc.add_layer(
            "Pause",
            vec![button(Color::rgb(90, 120, 200))],
            0,
            Transform::at(Point::new(1160.0, 850.0)),
        );
        doc.layer_mut(replay).unwrap().button = Some(WebAction::GotoAndPlay(0));
        doc.layer_mut(pause).unwrap().button = Some(WebAction::Stop);
        doc.set_frame_action(47, Some(WebAction::Stop));
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/web-check");
        std::fs::create_dir_all(&dir).unwrap();
        let settings = ExportSettings {
            format: crate::ExportFormat::Html,
            output: dir.join("buttons.html"),
            frames: 0..=47,
            scale: 0.5,
            transparent: false,
            project_dir: None,
        };
        write(&doc, &ImageLibrary::default(), &settings, |_| ControlFlow::Continue(())).unwrap();
    }
}
