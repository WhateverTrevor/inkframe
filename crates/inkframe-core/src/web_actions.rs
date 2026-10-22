//! What a web page export does as it plays and when it's clicked, as
//! Animate's ActionScript and HTML5 Canvas actions: a frame can stop the
//! animation or send it to another frame (`stop()`, `gotoAndPlay()`), and
//! a layer can be a button that does one of those, or opens a link, when
//! clicked. They only act in the exported page (see `html.rs` in the
//! exporter); in the editor the animation plays straight through.

use serde::{Deserialize, Serialize};

/// Something the page does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WebAction {
    Play,
    Stop,
    /// Goes to a frame (0-based, of the main timeline) and plays on.
    GotoAndPlay(u32),
    /// Goes to a frame and stops there.
    GotoAndStop(u32),
    /// Opens a web address.
    OpenLink(String),
}

impl WebAction {
    pub fn label(&self) -> String {
        match self {
            WebAction::Play => "Play".into(),
            WebAction::Stop => "Stop".into(),
            WebAction::GotoAndPlay(f) => format!("Go to frame {} and play", f + 1),
            WebAction::GotoAndStop(f) => format!("Go to frame {} and stop", f + 1),
            WebAction::OpenLink(url) => format!("Open {url}"),
        }
    }
}

/// What the page does on reaching `frame`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameAction {
    pub frame: u32,
    pub action: WebAction,
}

impl crate::Document {
    /// The action on `frame` of the main timeline, if any.
    pub fn frame_action(&self, frame: u32) -> Option<&WebAction> {
        self.frame_actions.iter().find(|a| a.frame == frame).map(|a| &a.action)
    }

    /// Puts `action` on `frame`, or takes the one there off with `None`.
    pub fn set_frame_action(&mut self, frame: u32, action: Option<WebAction>) {
        self.frame_actions.retain(|a| a.frame != frame);
        if let Some(action) = action {
            self.frame_actions.push(FrameAction { frame, action });
            self.frame_actions.sort_by_key(|a| a.frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_holds_one_action() {
        let mut doc = crate::Document::default();
        doc.set_frame_action(10, Some(WebAction::Stop));
        doc.set_frame_action(3, Some(WebAction::GotoAndPlay(0)));
        doc.set_frame_action(10, Some(WebAction::GotoAndStop(5)));
        assert_eq!(doc.frame_actions.iter().map(|a| a.frame).collect::<Vec<_>>(), [3, 10]);
        assert_eq!(doc.frame_action(10), Some(&WebAction::GotoAndStop(5)));
        doc.set_frame_action(3, None);
        assert_eq!(doc.frame_action(3), None);
    }
}
