//! Application state and route-to-content selection.
#[cfg(feature = "browser-tests")]
use crate::pages::Article;
use crate::pages::{ArticleRoute, Home, NotFound};
use crate::routes::{Page, href};
#[cfg(feature = "browser-tests")]
use fusor::prelude::*;
#[cfg(feature = "browser-tests")]
use fusor_router::browser::RouteContext;

pub(crate) struct App;

#[cfg(feature = "browser-tests")]
pub(crate) fn render_page(context: RouteContext<Page>) -> Content {
    match context.location.get().route {
        Some(Page::Home) => Content::new(|_| Home::default()),
        Some(Page::Article(_)) => {
            let location = context.location;
            Content::new(move |owner| Article::new(owner, location.clone()))
        }
        None => Content::new(|_| NotFound),
    }
}

fusor::bindings!(app);
