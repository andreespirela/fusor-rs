use fusor_async::{Data, ResourceState};
use std::{num::NonZeroUsize, rc::Rc, time::Duration};

#[derive(Clone, Copy, Debug)]
pub enum Freshness {
    For(Duration),
    Forever,
}

/// Every cache policy is explicit. Capacity bounds entry count (not value bytes).
/// Active entries are never silently evicted; new keys report `Capacity` when all
/// entries are pinned. Retry that subscription with `Query::refresh` after release.
#[derive(Clone, Copy, Debug)]
pub struct QueryOptions {
    pub freshness: Freshness,
    pub retention: Duration,
    pub capacity: NonZeroUsize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheInfo {
    pub entries: usize,
    pub observers: usize,
}

#[derive(Debug)]
pub enum QueryState<K, T, E> {
    Idle,
    Loading {
        key: K,
        previous: Option<Data<K, T>>,
    },
    Ready(Data<K, T>),
    Error {
        key: K,
        error: Rc<E>,
        previous: Option<Data<K, T>>,
    },
    /// All cache slots are held by active subscriptions. No request was started.
    Capacity {
        key: K,
    },
    Disposed,
}
impl<K: Clone, T, E> Clone for QueryState<K, T, E> {
    fn clone(&self) -> Self {
        match self {
            Self::Idle => Self::Idle,
            Self::Disposed => Self::Disposed,
            Self::Capacity { key } => Self::Capacity { key: key.clone() },
            Self::Loading { key, previous } => Self::Loading {
                key: key.clone(),
                previous: previous.clone(),
            },
            Self::Ready(data) => Self::Ready(data.clone()),
            Self::Error {
                key,
                error,
                previous,
            } => Self::Error {
                key: key.clone(),
                error: error.clone(),
                previous: previous.clone(),
            },
        }
    }
}
impl<K, T, E> QueryState<K, T, E> {
    pub fn data(&self) -> Option<&Data<K, T>> {
        match self {
            Self::Ready(data) => Some(data),
            Self::Loading { previous, .. } | Self::Error { previous, .. } => previous.as_ref(),
            _ => None,
        }
    }
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }
}
impl<K, T, E> From<ResourceState<K, T, E>> for QueryState<K, T, E> {
    fn from(state: ResourceState<K, T, E>) -> Self {
        match state {
            ResourceState::Idle => Self::Idle,
            ResourceState::Disposed => Self::Disposed,
            ResourceState::Loading { key, previous } => Self::Loading { key, previous },
            ResourceState::Ready(data) => Self::Ready(data),
            ResourceState::Error {
                key,
                error,
                previous,
            } => Self::Error {
                key,
                error,
                previous,
            },
        }
    }
}
