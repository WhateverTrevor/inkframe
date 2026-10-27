//! The Inkframe document model.
//!
//! This crate knows what an animation *is* — layers, shapes, keyframes and
//! how values move between keyframes — but nothing about how it is drawn or
//! edited. Keeping it free of UI and rendering dependencies means it can be
//! unit-tested quickly and reused by the renderer, the exporter, the editor
//! and (eventually) command-line tools.
//!
//! The main entry point is [`Document`].

pub mod abr;
mod adjust;
mod align;
mod anchor;
mod art_brush;
mod audio;
mod blend;
mod boolean;
mod brush;
mod camera;
mod channel;
mod clipboard;
mod clipping;
mod color;
mod container;
mod controller;
mod deform;
mod document;
mod draw_cache;
mod drawing;
mod easing;
mod effects;
mod envelope;
mod fill;
mod folder;
mod font_file;
mod frames;
mod freehand;
mod geometry;
mod guide;
mod history;
pub mod i18n;
mod ik;
mod image;
mod layer_mask;
mod library;
mod line;
mod lip_sync;
mod mask;
mod morph;
mod node_places;
mod paint;
mod palette;
mod parallel;
mod particles;
mod perspective;
mod preset;
mod psd;
mod retouch;
mod rig;
mod ruler_guides;
mod selection;
mod shape;
mod skeleton;
mod switch;
mod symbol;
mod template;
mod text;
mod track;
mod transform;
mod vector_fill;
mod vectorise;
mod warp;
mod web_actions;
mod weld;

pub use adjust::{Adjustment, adjust};
pub use align::{Align, Axis};
pub use anchor::{
    Anchor, KAPPA, PathHit, anchors_from_path, insert_anchor, nearest_on_anchors, path_from_anchors,
};
pub use art_brush::{BrushKind, StrokeBrush};
pub use audio::{AudioId, AudioTrack};
pub use blend::BlendMode;
pub use boolean::{Area, CombineError, ShapeOp, area_of, break_apart, combine};
pub use brush::{
    Blend, BrushMode, BrushPose, BrushPreset, BrushSettings, BrushStroke, ColorDynamics, Control, DabInput,
    DabPath, DualBrush, Pattern, PatternPaint, Pen, Scattering, ShapeDynamics, Symmetry, Texture, Tip,
    TipImage, Transfer,
};
pub use camera::FOCAL_LENGTH;
pub use channel::{Animatable, AnyChannel, Bend, Channel, Key, MotionCurve, Tween};
pub use clipboard::{Clipboard, CopiedFrames, CopiedKeyframe, CopiedLayers, CopiedRow, CopiedShapes};
pub use color::{Color, ParseColorError, hsv_to_rgb, rgb_to_hsv};
pub use controller::{Controller, ControllerPose};
pub use deform::Deformer;
pub use document::{Document, DocumentError, Element, FORMAT_VERSION, FrameRate, Layer, LayerId, TimelineId};
pub use draw_cache::{DrawCache, Posed, with_draw_cache, with_pose_cache};
pub use drawing::{Drawing, Exposure};
pub use easing::Easing;
pub use effects::{
    ColorEffect, Effects, Filter, FilterKind, apply_color_effect, apply_filters, filter_margin,
};
pub use envelope::Lattice;
pub use fill::{BucketSettings, Gradient, GradientShape, bucket_fill, gradient};
pub use folder::TAG_COLORS;
pub use font_file::{FontFile, FontFileId};
pub use freehand::fit_freehand_path;
pub use geometry::{Affine, Point, Rect};
pub use guide::GuidePath;
pub use history::History;
pub use ik::RotationLimits;
pub use image::{ImageAsset, ImageId};
pub use layer_mask::{LayerMask, mask_coverage};
pub use library::Asset;
pub use line::{LineTexture, TextureKind, centreline_anchors, line_outline, with_anchors_every};
pub use lip_sync::{
    LipSyncError, MouthCue, MouthMap, MouthShape, cues_from_loudness, parse_papagayo, parse_rhubarb,
    parse_timing,
};
pub use morph::{ShapeTween, morph};
pub use node_places::{NodePlace, NodeRef};
pub use paint::{Canvas, CanvasId, TILE, premultiply, unpremultiply};
pub use palette::{Palette, Swatch};
pub use particles::{Emitter, EmitterShape, MAX_PARTICLES, Particle, Preset, Sprite};
pub use perspective::{Perspective, PerspectiveKind};
pub use preset::{MotionPreset, PRESET_EXTENSION, PresetKey};
pub use psd::{Psd, PsdError, PsdLayer};
pub use retouch::{Retouch, RetouchStroke, ToneRange};
pub use ruler_guides::{RulerGuide, snap_box, snap_point, snap_value};
pub use selection::{Combine, Selection};
pub use shape::{Dash, GradientStop, LineCap, LineJoin, Path, Segment, Shape, ShapeGradient, Stroke, Style};
pub use skeleton::{
    Bone, BoneBinding, BonePoints, CorrectionKey, Dynamics, LayerBend, MESH_CELL, Pose, RestPart,
    RestPoseView, ShapeBones, Skeleton, SmartAction, TurnPreview, corrected, mesh_lines,
};
pub use switch::SwitchKey;
pub use symbol::{ConvertError, Instance, MAX_SYMBOL_DEPTH, Playback, Symbol, SymbolId, SymbolSwap};
pub use template::{TEMPLATE_EXTENSION, Template};
pub use text::{DEFAULT_FAMILY, Font, Text, TextAlign};
pub use track::{Property, Track};
pub use transform::Transform;
pub use vector_fill::{FillResult, enclosed_area};
pub use vectorise::{Bitmap, TraceMode, TraceOptions, Traced, trace};
pub use warp::Warp;
pub use web_actions::{FrameAction, WebAction};
pub use weld::{WELD_DISTANCE, welded_at, welded_points, welded_to};
