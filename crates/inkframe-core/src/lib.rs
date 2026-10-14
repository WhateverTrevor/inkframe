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
mod audio;
mod channel;
mod color;
mod document;
mod easing;
mod font_file;
mod freehand;
mod geometry;
mod guide;
mod history;
mod image;
mod mask;
mod shape;
mod symbol;
mod text;
mod track;
mod transform;

pub use anchor::{
    Anchor, KAPPA, PathHit, anchors_from_path, insert_anchor, nearest_on_anchors, path_from_anchors,
};
pub use audio::{AudioId, AudioTrack};
pub use channel::{Animatable, AnyChannel, Bend, Channel, Key, MotionCurve, Tween};
pub use color::{Color, ParseColorError};
pub use document::{Document, DocumentError, Element, FORMAT_VERSION, FrameRate, Layer, LayerId, TimelineId};
pub use easing::Easing;
pub use font_file::{FontFile, FontFileId};
pub use freehand::fit_freehand_path;
pub use geometry::{Affine, Point, Rect};
pub use guide::GuidePath;
pub use history::History;
pub use image::{ImageAsset, ImageId};
pub use shape::{Path, Segment, Shape, Stroke, Style};
pub use symbol::{Instance, MAX_SYMBOL_DEPTH, Playback, Symbol, SymbolId};
pub use text::{DEFAULT_FAMILY, Font, Text, TextAlign};
pub use track::{Property, Track};
pub use transform::Transform;
