//! Hand server-rendered DOM from a structural binding to the generated mount
//! it prepares next. That mount takes the offer as its first step; a mount of
//! the other shape leaves it alone, and the previous offer returns afterwards.
use super::{MountPoint, scoped};
use std::cell::RefCell;
#[cfg(feature = "islands")]
use web_sys::Element;

enum Target {
    /// The root element of a component, adopted by a root-template mount.
    #[cfg(feature = "islands")]
    Root(Element),
    /// The range holding a fragment, adopted by a fragment mount.
    Range(MountPoint),
}

thread_local! {
    static OFFER: RefCell<Option<Target>> = const { RefCell::new(None) };
}

#[cfg(feature = "islands")]
pub(super) fn with_root<R>(root: &Element, run: impl FnOnce() -> R) -> R {
    scoped(&OFFER, Some(Target::Root(root.clone())), run)
}

/// `None` withdraws any outer offer while `run` prepares detached DOM.
pub(super) fn with_range<R>(range: Option<MountPoint>, run: impl FnOnce() -> R) -> R {
    scoped(&OFFER, range.map(Target::Range), run)
}

#[cfg(feature = "islands")]
pub(super) fn take_root() -> Option<Element> {
    OFFER.with_borrow_mut(|offer| match offer.take() {
        Some(Target::Root(root)) => Some(root),
        other => {
            *offer = other;
            None
        }
    })
}

pub(super) fn take_range() -> Option<MountPoint> {
    OFFER.with_borrow_mut(|offer| match offer.take() {
        Some(Target::Range(range)) => Some(range),
        other => {
            *offer = other;
            None
        }
    })
}
