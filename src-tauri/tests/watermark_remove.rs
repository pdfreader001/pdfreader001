//! Watermark removal (M4 part 2) tests.
//!
//! Tests the core helpers in `watermark_remove`. For end-to-end coverage we
//! exercise `Rect::valid`, `rects_intersect`, and `quantize` plus the
//! `ObjectFingerprint` constructor shape.

use pdfe_lib::watermark_remove::{quantize, rects_intersect, ObjectFingerprint, Rect};

#[test]
fn rect_validates_dimensions() {
    assert!(Rect {
        left: 0.0,
        bottom: 0.0,
        right: 10.0,
        top: 10.0
    }
    .valid());
    // Zero-width or zero-height rectangle should be invalid.
    assert!(!Rect {
        left: 5.0,
        bottom: 0.0,
        right: 5.0,
        top: 10.0
    }
    .valid());
    assert!(!Rect {
        left: 0.0,
        bottom: 5.0,
        right: 10.0,
        top: 5.0
    }
    .valid());
}

#[test]
fn rect_intersection_overlap() {
    let a = Rect {
        left: 0.0,
        bottom: 0.0,
        right: 100.0,
        top: 100.0,
    };
    let b = Rect {
        left: 50.0,
        bottom: 50.0,
        right: 150.0,
        top: 150.0,
    };
    let c = Rect {
        left: 200.0,
        bottom: 200.0,
        right: 300.0,
        top: 300.0,
    };
    assert!(rects_intersect(&a, &b));
    assert!(rects_intersect(&b, &a));
    assert!(!rects_intersect(&a, &c));
}

#[test]
fn quantize_grid_snap() {
    // 5-point grid: 0..2.5 -> 0, 2.5..7.5 -> 1, etc.
    assert_eq!(quantize(0.0, 5.0), 0);
    assert_eq!(quantize(2.4, 5.0), 0);
    assert_eq!(quantize(3.0, 5.0), 1);
    assert_eq!(quantize(7.5, 5.0), 2);
    assert_eq!(quantize(-3.0, 5.0), -1);
}

#[test]
fn fingerprint_constructor_fields() {
    let fp = ObjectFingerprint {
        object_index: 7,
        kind: "text".to_string(),
        left: 10.0,
        bottom: 20.0,
        right: 100.0,
        top: 60.0,
        occurrence: 5,
        total_sampled: 10,
        key: None,
    };
    assert_eq!(fp.object_index, 7);
    assert_eq!(fp.kind, "text");
    assert_eq!(fp.left, 10.0);
    assert_eq!(fp.bottom, 20.0);
    assert_eq!(fp.right, 100.0);
    assert_eq!(fp.top, 60.0);
    assert_eq!(fp.occurrence, 5);
    assert_eq!(fp.total_sampled, 10);
    assert_eq!(fp.key, None);
}
