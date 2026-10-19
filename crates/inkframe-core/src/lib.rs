//! The Inkframe document model.
//!
//! This crate knows what an animation *is* — layers, shapes, keyframes and
//! how values move between keyframes — but nothing about how it is drawn or
//! edited. Keeping it free of UI and rendering dependencies means it can be
//! unit-tested quickly and reused by the renderer, the exporter, the editor
//! and (eventually) command-line tools.
//!
//! The main entry point is [`Document`].

mod adjust;
mod align;
mod anchor;
mod audio;
mod blend;
mod brush;
mod camera;
mod channel;
mod clipboard;
mod color;
mod container;
mod deform;
mod document;
mod drawing;
mod easing;
mod effects;
mod fill;
mod folder;
mod font_file;
mod freehand;
mod geometry;
mod guide;
mod history;
mod ik;
mod image;
mod lip_sync;
mod mask;
mod morph;
mod paint;
mod palette;
mod psd;
mod retouch;
mod rig;
mod selection;
mod shape;
mod symbol;
mod text;
mod track;
mod transform;
mod vector_fill;
mod warp;

pub use adjust::{Adjustment, adjust};
pub use align::{Align, Axis};
pub use anchor::{
    Anchor, KAPPA, PathHit, anchors_from_path, insert_anchor, nearest_on_anchors, path_from_anchors,
};
pub use audio::{AudioId, AudioTrack};
pub use blend::BlendMode;
pub use brush::{BrushMode, BrushSettings, BrushStroke, DabPath};
pub use camera::FOCAL_LENGTH;
pub use channel::{Animatable, AnyChannel, Bend, Channel, Key, MotionCurve, Tween};
pub use clipboard::{Clipboard, CopiedKeyframe, CopiedLayers};
pub use color::{Color, ParseColorError};
pub use deform::Deformer;
pub use document::{Document, DocumentError, Element, FORMAT_VERSION, FrameRate, Layer, LayerId, TimelineId};
pub use drawing::{Drawing, Exposure};
pub use easing::Easing;
pub use effects::{
    ColorEffect, Effects, Filter, FilterKind, apply_color_effect, apply_filters, filter_margin,
};
pub use fill::{BucketSettings, Gradient, GradientShape, bucket_fill, gradient};
pub use folder::TAG_COLORS;
pub use font_file::{FontFile, FontFileId};
pub use freehand::fit_freehand_path;
pub use geometry::{Affine, Point, Rect};
pub use guide::GuidePath;
pub use history::History;
pub use ik::RotationLimits;
pub use image::{ImageAsset, ImageId};
pub use lip_sync::{
    LipSyncError, MouthCue, MouthMap, MouthShape, cues_from_loudness, parse_papagayo, parse_rhubarb,
};
pub use morph::{ShapeTween, morph};
pub use paint::{Canvas, CanvasId, TILE, premultiply, unpremultiply};
pub use palette::{Palette, Swatch};
pub use psd::{Psd, PsdError, PsdLayer};
pub use retouch::{Retouch, RetouchStroke, ToneRange};
pub use selection::{Combine, Selection};
pub use shape::{Path, Segment, Shape, Stroke, Style};
pub use symbol::{Instance, MAX_SYMBOL_DEPTH, Playback, Symbol, SymbolId};
pub use text::{DEFAULT_FAMILY, Font, Text, TextAlign};
pub use track::{Property, Track};
pub use transform::Transform;
pub use vector_fill::{FillResult, enclosed_area};
pub use warp::Warp;
