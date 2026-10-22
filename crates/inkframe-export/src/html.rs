//! A web page that plays the animation, as Animate's HTML5 Canvas export
//! does: one self-contained `.html` file to open in any browser or put on
//! a site, with nothing else to load.
//!
//! Every frame is drawn as SVG (see `svg.rs`), so the page stays sharp at
//! any size, and frames that look the same are kept once. A few lines of
//! script swap them at the document's frame rate, looping; clicking the
//! animation pauses and plays it, and the arrow keys step a frame when
//! paused. It fits the window, keeping its shape.

use std::fmt::Write;
use std::ops::ControlFlow;

use inkframe_core::Document;
use inkframe_render::ImageLibrary;

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
    std::fs::write(
        &settings.output,
        page(&title, &frames, doc.frame_rate.fps(), doc.background, settings.transparent),
    )?;
    Ok(())
}

/// The page: the frames (each different one once, and which to show
/// when), and the script that plays them.
pub fn page(
    title: &str,
    frames: &[String],
    fps: f64,
    background: inkframe_core::Color,
    transparent: bool,
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
    let data =
        serde_json::json!({ "fps": fps, "frames": unique, "order": order }).to_string().replace("</", "<\\/");
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
  let frame = 0, playing = true, shown = -1, last = performance.now(), owed = 0;
  const show = () => {{
    const which = animation.order[frame];
    if (which !== shown) {{ stage.innerHTML = animation.frames[which]; shown = which; }}
  }};
  const tick = (now) => {{
    // Frames advance by time, not by screen refreshes, so the timing holds
    // on any display.
    if (playing) {{
      owed += (now - last) * animation.fps / 1000;
      const steps = Math.floor(owed);
      if (steps > 0) {{ frame = (frame + steps) % count; owed -= steps; show(); }}
    }}
    last = now;
    requestAnimationFrame(tick);
  }};
  stage.addEventListener("click", () => {{ playing = !playing; owed = 0; }});
  document.addEventListener("keydown", (event) => {{
    if (playing) return;
    if (event.key === "ArrowRight") frame = (frame + 1) % count;
    else if (event.key === "ArrowLeft") frame = (frame + count - 1) % count;
    else return;
    show();
  }});
  show();
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
        let html = page("Walk", &frames, 24.0, inkframe_core::Color::WHITE, false);
        assert!(html.contains(r#""order":[0,0,1,0]"#), "{html}");
        assert_eq!(html.matches("<svg>a").count(), 1);
        assert!(!html.contains("</svg>\""), "closing tags are broken up inside the script");
        assert!(html.contains("<title>Walk</title>"));
        assert!(html.contains("rgb(255, 255, 255)"));
    }
}
