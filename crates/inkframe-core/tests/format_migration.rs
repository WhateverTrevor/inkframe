//! Files saved by older versions of Inkframe must keep opening and playing
//! back exactly as they did. The fixture here is the original sample file,
//! saved in format version 1 (whole-transform keyframes).

use inkframe_core::{Document, Easing, FORMAT_VERSION, Property};

const V1_SAMPLE: &str = include_str!("fixtures/bouncing-ball-v1.inkf");

#[test]
fn version_1_files_open_and_upgrade() {
    let doc = Document::from_json(V1_SAMPLE).expect("v1 file should load");
    assert_eq!(doc.version, FORMAT_VERSION);
    assert_eq!(doc.layers.len(), 2);

    // Every old keyframe became a key on every transform property
    // (version 1 had no effects or depth).
    let ball = &doc.layers[1];
    for property in Property::ALL.into_iter().filter(|&p| !matches!(p, Property::Effects | Property::Depth)) {
        assert_eq!(ball.track.channel(property).key_frames(), [0, 12, 24, 36, 47], "{property:?}");
    }
}

#[test]
fn version_1_files_play_back_unchanged() {
    let doc = Document::from_json(V1_SAMPLE).unwrap();
    let ball = &doc.layers[1];

    // Halfway through the first fall, version 1 eased every property with
    // the first keyframe's Ease In. Recompute that by hand and compare.
    let eased = Easing::EaseIn.apply(0.5);
    let pose = ball.transform_at(6);
    assert!((pose.position.y - (260.0 + (894.0 - 260.0) * eased)).abs() < 1e-3);
    assert!((pose.scale.x - (1.0 + 0.3 * eased)).abs() < 1e-5);
    assert!((pose.scale.y - (1.0 - 0.25 * eased)).abs() < 1e-5);
}

#[test]
fn upgraded_files_round_trip_in_the_new_format() {
    let doc = Document::from_json(V1_SAMPLE).unwrap();
    let json = doc.to_json().unwrap();
    assert!(json.contains("\"position\": ["), "should be saved with per-property channels");
    assert_eq!(Document::from_json(&json).unwrap(), doc);
}
