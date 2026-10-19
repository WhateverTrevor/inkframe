//! Templates, as Harmony's library: layers saved on their own (a rigged
//! character, a walk cycle, a prop) to drop into any scene.
//!
//! A template is what copying the layers puts on the clipboard (see
//! `clipboard.rs`): the layers with their animation, and every symbol,
//! picture, font and painted canvas they use. It also notes the frame
//! its animation started on, so inserting it at another frame moves the
//! whole thing there. Templates are files of their own, outside any
//! project, so every project can use them.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::clipboard::CopiedLayers;
use crate::document::{Document, DocumentError, FORMAT_VERSION, LayerId, TimelineId};
use crate::geometry::Point;

/// The file extension templates are saved with.
pub const TEMPLATE_EXTENSION: &str = "inktemplate";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    /// The format version it was saved with, as for documents.
    pub version: u32,
    pub name: String,
    /// The frame its layers' animation started on when it was saved.
    pub first_frame: u32,
    pub layers: CopiedLayers,
}

impl Template {
    pub fn to_json(&self) -> Result<String, DocumentError> {
        Ok(serde_json::to_string(self)?)
    }

    /// Reads a template, refusing one saved by a newer Inkframe.
    pub fn from_json(json: &str) -> Result<Self, DocumentError> {
        let template: Template = serde_json::from_str(json)?;
        if template.version > FORMAT_VERSION {
            return Err(DocumentError::UnsupportedVersion { found: template.version });
        }
        Ok(template)
    }
}

impl Document {
    /// A template of the layers `ids` (with everything they need).
    pub fn make_template(
        &self,
        name: impl Into<String>,
        ids: &[LayerId],
        project_dir: Option<&Path>,
    ) -> Option<Template> {
        let layers = self.copy_layers(ids, project_dir)?;
        let first_frame = layers.layers.iter().map(|l| l.start).min().unwrap_or(0);
        Some(Template { version: FORMAT_VERSION, name: name.into(), first_frame, layers })
    }

    /// Adds a template's layers on top of `timeline`, where they were on
    /// the stage, with their animation moved to start on `frame`. Returns
    /// the new layers.
    pub fn insert_template(
        &mut self,
        template: &Template,
        timeline: TimelineId,
        frame: u32,
        project_dir: Option<&Path>,
    ) -> Vec<LayerId> {
        let mut layers = template.layers.clone();
        let by = frame as i64 - template.first_frame as i64;
        for layer in &mut layers.layers {
            layer.shift_time(by);
        }
        self.paste_layers(&layers, timeline, Point::ZERO, project_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Element, Shape, Style, Transform};

    /// A document with a ball that moves from frame 0 to frame 10.
    fn ball() -> (Document, LayerId) {
        let mut doc = Document::default();
        let dot = Element {
            shape: Shape::Ellipse { radius_x: 10.0, radius_y: 10.0 },
            style: Style { fill: Some(Color::BLACK), stroke: None },
        };
        let id = doc.add_layer("Ball", vec![dot], 0, Transform::at(Point::new(0.0, 0.0)));
        doc.layer_mut(id).unwrap().track.set_pose(10, Transform::at(Point::new(100.0, 0.0)));
        (doc, id)
    }

    #[test]
    fn a_template_inserted_later_plays_from_there() {
        let (doc, id) = ball();
        let template = doc.make_template("Bounce", &[id], None).unwrap();
        let json = template.to_json().unwrap();
        let template = Template::from_json(&json).unwrap();

        let mut other = Document::default();
        let new = other.insert_template(&template, TimelineId::Main, 20, None);
        let layer = other.layer(new[0]).unwrap();
        assert_eq!(layer.start, 20);
        assert_eq!(layer.track.key_frames(), [20, 30]);
        assert_eq!(layer.track.sample(25.0).position, Point::new(50.0, 0.0));
    }

    #[test]
    fn templates_from_newer_versions_are_refused() {
        let (doc, id) = ball();
        let mut template = doc.make_template("Bounce", &[id], None).unwrap();
        template.version = FORMAT_VERSION + 1;
        assert!(Template::from_json(&template.to_json().unwrap()).is_err());
    }
}
