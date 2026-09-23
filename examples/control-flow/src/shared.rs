use super::*;
pub struct Shared {
    pub(super) session: Signal<Session>,
    pub(super) visible: Signal<bool>,
}
impl Shared {
    pub fn new() -> Self {
        Self {
            session: signal(Session::Authenticated {
                user: User { name: "Ada".into() },
            }),
            visible: signal(true),
        }
    }
}
impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}
fusor::template!("web/components/shared.html");
