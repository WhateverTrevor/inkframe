//! The Inkframe document model.
//!
//! This crate knows what an animation *is* — layers, shapes, keyframes and
//! how values move between keyframes — but nothing about how it is drawn or
//! edited. Keeping it free of UI and rendering dependencies means it can be
//! unit-tested quickly and reused by the renderer, the exporter, the editor
//! and (eventually) command-line tools.
//!
//! The main entry point is [`Document`].

mod anchor;
mod color;
mod document;
mod easing;
mod freehand;
mod geometry;
mod history;
mod shape;
mod track;
mod transform;

pub use anchor::{Anchor, KAPPA, anchors_from_path, path_from_anchors};
pub use color::{Color, ParseColorError};
pub use document::{Document, DocumentError, Element, FORMAT_VERSION, FrameRate, Layer, LayerId};
pub use easing::Easing;
pub use freehand::fit_freehand_path;
pub use geometry::{Affine, Point, Rect};
pub use history::History;
pub use shape::{Path, Segment, Shape, Stroke, Style};
pub use track::{Keyframe, Track};
pub use transform::Transform;
