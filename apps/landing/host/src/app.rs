use crate::{async_data::AsyncData, counter::Counter, keyed_list::KeyedList, search::LiveSearch};

// The page keeps no state of its own; each component owns its state.
struct App;

fusor::template!("web/index.html");
