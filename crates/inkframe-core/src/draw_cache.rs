//! What drawing a frame works out once and shares: each skeleton's pose,
//! and each bent layer's shapes as posed.
//!
//! The renderer splits a frame into bands drawn in parallel, and each band
//! draws every layer that reaches it, so without this every band would
//! pose every skeleton (inverse kinematics, dynamics) and bend every layer
//! along it, over and over. Inside [`with_draw_cache`], the first to ask
//! works a thing out and the rest wait for it and share it: one cache for
//! the whole frame, across all the threads drawing it.
//!
//! Things are kept by where they are in memory, and the frame. That's only
//! safe while the document can't change, so a cache is for code that holds
//! the document borrowed, unchanging, throughout, as drawing a frame does,
//! and goes when that's done.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::{Element, Pose};

/// Things worked out once each, by what they belong to (where it is in
/// memory) and the frame: the first to ask works one out, and anyone else
/// asking meanwhile waits for it.
type Shared<V> = Mutex<HashMap<(usize, u32), Arc<OnceLock<V>>>>;

/// Poses and posed shapes worked out while drawing one frame (or several,
/// for onion skins and motion blur), by what they belong to and the frame.
#[derive(Default)]
pub struct DrawCache {
    poses: Shared<Pose>,
    /// `None` for a layer whose shapes aren't changed by posing, which are
    /// then borrowed from it as they are.
    posed: Shared<Option<Arc<Vec<Element>>>>,
}

thread_local! {
    /// The cache this thread is drawing with, if any.
    static CURRENT: RefCell<Option<Arc<DrawCache>>> = const { RefCell::new(None) };
}

/// Runs `f` drawing with `cache`, shared with any other threads given the
/// same one. A cache already in use on this thread carries on instead. The
/// thread stops using it when `f` returns (or panics).
pub fn with_draw_cache<R>(cache: &Arc<DrawCache>, f: impl FnOnce() -> R) -> R {
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            CURRENT.with(|current| *current.borrow_mut() = None);
        }
    }
    let entered = CURRENT.with(|current| {
        let mut current = current.borrow_mut();
        let enter = current.is_none();
        if enter {
            *current = Some(Arc::clone(cache));
        }
        enter
    });
    let _leave = entered.then_some(Leave);
    f()
}

/// Runs `f` with a cache of its own, for drawing on one thread.
pub fn with_pose_cache<R>(f: impl FnOnce() -> R) -> R {
    with_draw_cache(&Arc::default(), f)
}

/// `compute`d once per key in the cache in use, or straight away outside
/// one. Others asking for the same key meanwhile wait for it rather than
/// working it out too.
fn cached<V: Clone>(
    map: impl Fn(&DrawCache) -> &Shared<V>,
    key: (usize, u32),
    compute: impl FnOnce() -> V,
) -> V {
    let cell = CURRENT.with(|current| {
        current
            .borrow()
            .as_ref()
            .map(|cache| Arc::clone(map(cache).lock().expect("not poisoned").entry(key).or_default()))
    });
    match cell {
        Some(cell) => cell.get_or_init(compute).clone(),
        None => compute(),
    }
}

pub(crate) fn pose(skeleton: usize, frame: u32, compute: impl FnOnce() -> Pose) -> Pose {
    cached(|c| &c.poses, (skeleton, frame), compute)
}

pub(crate) fn posed(
    layer: usize,
    frame: u32,
    compute: impl FnOnce() -> Option<Arc<Vec<Element>>>,
) -> Option<Arc<Vec<Element>>> {
    cached(|c| &c.posed, (layer, frame), compute)
}

/// A layer's shapes as posed (see [`crate::Document::posed_shapes`]): its own,
/// or bent ones shared from the cache.
pub enum Posed<'a> {
    Own(std::borrow::Cow<'a, [Element]>),
    Shared(Arc<Vec<Element>>),
}

impl std::ops::Deref for Posed<'_> {
    type Target = [Element];
    fn deref(&self) -> &[Element] {
        match self {
            Posed::Own(elements) => elements,
            Posed::Shared(elements) => elements,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threads_sharing_a_cache_work_each_thing_out_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let cache = Arc::new(DrawCache::default());
        let worked_out = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    with_draw_cache(&cache, || {
                        for frame in 0..3 {
                            pose(1, frame, || {
                                worked_out.fetch_add(1, Ordering::Relaxed);
                                std::thread::sleep(std::time::Duration::from_millis(5));
                                Pose::default()
                            });
                        }
                    })
                });
            }
        });
        assert_eq!(worked_out.load(Ordering::Relaxed), 3, "once per frame, not per thread");
        // Outside a cache, nothing's kept.
        pose(1, 0, || {
            worked_out.fetch_add(1, Ordering::Relaxed);
            Pose::default()
        });
        assert_eq!(worked_out.load(Ordering::Relaxed), 4);
    }
}
